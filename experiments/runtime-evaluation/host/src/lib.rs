use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use anyhow::Result;
use tuilane_core::{CommandCallerId, CommandDispatchError, CommandDispatcher};
use tuilane_plugin_api::{Command, CommandDescriptor, CommandEffect, CommandPermission};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store, StoreLimits, StoreLimitsBuilder};

pub mod bindings {
    wasmtime::component::bindgen!({
        path: "../wit",
        world: "evaluation",
    });
}

const ECHO_PERMISSION: CommandPermission = CommandPermission::new("evaluation.read");
const WRITE_PERMISSION: CommandPermission = CommandPermission::new("evaluation.write");
pub const MEMORY_LIMIT_BYTES: usize = 32 * 1024 * 1024;
const MAX_OPERATION_LENGTH: usize = 128;
const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

const ECHO_DESCRIPTOR: CommandDescriptor = CommandDescriptor::new(
    "evaluation.echo@1",
    "Echo a payload through the host command dispatcher.",
    "bytes",
    "bytes",
    CommandEffect::ReadOnly,
    ECHO_PERMISSION,
);
const WRITE_DESCRIPTOR: CommandDescriptor = CommandDescriptor::new(
    "evaluation.write@1",
    "Attempt a mutating operation through the host command dispatcher.",
    "bytes",
    "bytes",
    CommandEffect::Mutating,
    WRITE_PERMISSION,
);

#[derive(Debug)]
struct EvaluationCommand {
    operation: EvaluationOperation,
    payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
enum EvaluationOperation {
    Echo,
    Write,
}

impl Command for EvaluationCommand {
    type Response = Vec<u8>;

    const NAME: &'static str = "runtime-evaluation";
    const DESCRIPTORS: &'static [CommandDescriptor] = &[ECHO_DESCRIPTOR, WRITE_DESCRIPTOR];

    fn descriptor(&self) -> &'static CommandDescriptor {
        match self.operation {
            EvaluationOperation::Echo => &ECHO_DESCRIPTOR,
            EvaluationOperation::Write => &WRITE_DESCRIPTOR,
        }
    }
}

/// Command effects and counters used by the runtime experiment.
#[derive(Clone, Default)]
pub struct EvaluationCounters {
    reads: Arc<AtomicUsize>,
    writes: Arc<AtomicUsize>,
}

impl EvaluationCounters {
    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
    }

    pub fn writes(&self) -> usize {
        self.writes.load(Ordering::Relaxed)
    }
}

/// Host state for one guest instance. Caller identity is assigned by the host.
pub struct EvaluationHost {
    dispatcher: CommandDispatcher,
    caller: CommandCallerId,
    counters: EvaluationCounters,
    limits: StoreLimits,
}

impl EvaluationHost {
    pub fn new() -> Result<Self> {
        let caller = CommandCallerId::new("evaluation-plugin")?;
        let counters = EvaluationCounters::default();
        let mut dispatcher = CommandDispatcher::new();

        let read_counter = counters.reads.clone();
        let write_counter = counters.writes.clone();
        dispatcher.register::<EvaluationCommand, _, std::convert::Infallible>(move |command| {
            match command.operation {
                EvaluationOperation::Echo => {
                    read_counter.fetch_add(1, Ordering::Relaxed);
                    Ok(command.payload)
                }
                EvaluationOperation::Write => {
                    write_counter.fetch_add(1, Ordering::Relaxed);
                    Ok(command.payload)
                }
            }
        })?;
        dispatcher.grant_permission(&caller, ECHO_PERMISSION);

        Ok(Self {
            dispatcher,
            caller,
            counters,
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY_LIMIT_BYTES)
                .build(),
        })
    }

    pub fn counters(&self) -> EvaluationCounters {
        self.counters.clone()
    }

    pub fn dispatch(
        &mut self,
        operation: &str,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, bindings::tuilane::runtime_evaluation::host::HostError> {
        use bindings::tuilane::runtime_evaluation::host::HostError;

        if operation.len() > MAX_OPERATION_LENGTH || payload.len() > MAX_PAYLOAD_BYTES {
            return Err(HostError::InvalidRequest);
        }

        let operation = match operation {
            "evaluation.echo@1" => EvaluationOperation::Echo,
            "evaluation.write@1" => EvaluationOperation::Write,
            _ => return Err(HostError::UnknownOperation),
        };
        let command = EvaluationCommand { operation, payload };
        self.dispatcher
            .execute_as(&self.caller, command)
            .into_result()
            .map_err(|error| match error {
                CommandDispatchError::PermissionDenied { .. } => HostError::Denied,
                _ => HostError::Failed,
            })
    }
}

impl bindings::tuilane::runtime_evaluation::host::Host for EvaluationHost {
    fn dispatch(
        &mut self,
        operation: String,
        payload: Vec<u8>,
    ) -> std::result::Result<Vec<u8>, bindings::tuilane::runtime_evaluation::host::HostError> {
        EvaluationHost::dispatch(self, &operation, payload)
    }
}

pub fn engine() -> Result<Engine> {
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    Engine::new(&config).map_err(|error| anyhow::anyhow!("create Wasmtime engine: {error}"))
}

pub fn linker(engine: &Engine) -> Result<Linker<EvaluationHost>> {
    let mut linker = Linker::new(engine);
    bindings::Evaluation::add_to_linker::<_, HasSelf<_>>(&mut linker, |host| host)
        .map_err(|error| anyhow::anyhow!("link WIT host interface: {error}"))?;
    Ok(linker)
}

pub fn instance(
    engine: &Engine,
    component: &Component,
    host: EvaluationHost,
) -> Result<(Store<EvaluationHost>, bindings::Evaluation)> {
    let linker = linker(engine)?;
    let mut store = Store::new(engine, host);
    store.limiter(|host| &mut host.limits);
    store.set_epoch_deadline(1);
    store
        .set_fuel(10_000_000)
        .map_err(|error| anyhow::anyhow!("set guest fuel limit: {error}"))?;
    let bindings = bindings::Evaluation::instantiate(&mut store, component, &linker)
        .map_err(|error| anyhow::anyhow!("instantiate evaluation component: {error}"))?;
    Ok((store, bindings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bindings::tuilane::runtime_evaluation::host::HostError;

    #[test]
    fn dispatcher_denies_ungranted_mutation_before_handler() {
        let mut host = EvaluationHost::new().unwrap();
        let counters = host.counters();

        assert_eq!(
            host.dispatch("evaluation.echo@1", b"read".to_vec())
                .unwrap(),
            b"read"
        );
        assert_eq!(
            host.dispatch("evaluation.write@1", b"write".to_vec()),
            Err(HostError::Denied)
        );
        assert_eq!(counters.reads(), 1);
        assert_eq!(counters.writes(), 0);
    }

    #[test]
    fn dispatcher_rejects_unknown_and_oversized_requests() {
        let mut host = EvaluationHost::new().unwrap();

        assert_eq!(
            host.dispatch("evaluation.missing@1", Vec::new()),
            Err(HostError::UnknownOperation)
        );
        assert_eq!(
            host.dispatch("evaluation.echo@1", vec![0; MAX_PAYLOAD_BYTES + 1]),
            Err(HostError::InvalidRequest)
        );
    }
}
