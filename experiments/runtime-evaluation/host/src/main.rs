use std::{
    env, fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use tuilane_runtime_evaluation_host::{
    EvaluationHost, MEMORY_LIMIT_BYTES, bindings, engine, instance,
};
use wasmtime::component::Component;

const STARTUP_SAMPLES: usize = 5;
const LATENCY_SAMPLES: usize = 5_000;
const ECHO_OPERATION: &str = "evaluation.echo@1";
const WRITE_OPERATION: &str = "evaluation.write@1";
const PAYLOAD: &[u8] = b"runtime-evaluation";

fn main() -> Result<()> {
    let component_path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| env::var_os("TUILANE_RUNTIME_EVALUATION_COMPONENT").map(PathBuf::from))
        .context("pass the component path or set TUILANE_RUNTIME_EVALUATION_COMPONENT")?;

    print_rss("process baseline");
    let startup = measure_cold_startup(&component_path)?;
    let engine = engine()?;

    let compile_started = Instant::now();
    let component = Component::from_file(&engine, &component_path)
        .map_err(|error| anyhow::anyhow!("compile component: {error}"))?;
    println!(
        "Component compile/load: {}",
        format_duration(compile_started.elapsed())
    );
    print_rss("engine and component loaded");

    let (counters, first_call, guest_memory) = {
        let host = EvaluationHost::new()?;
        let counters = host.counters();
        let instantiate_started = Instant::now();
        let (mut store, bindings) = instance(&engine, &component, host)?;
        println!(
            "Instance creation: {}",
            format_duration(instantiate_started.elapsed())
        );
        print_rss("component instance created");

        let plugin = bindings.tuilane_runtime_evaluation_plugin();
        let first_call_started = Instant::now();
        let metadata = plugin
            .call_metadata(&mut store)
            .map_err(|error| anyhow::anyhow!("read plugin metadata: {error}"))?;
        let first_call = first_call_started.elapsed();
        ensure!(metadata.id == "evaluation.echo", "unexpected plugin ID");
        ensure!(metadata.version == "0.1.0", "unexpected plugin version");

        let echo = plugin
            .call_call(&mut store, ECHO_OPERATION, PAYLOAD)
            .map_err(|error| anyhow::anyhow!("invoke guest call export: {error}"))?;
        ensure!(
            echo == Ok(PAYLOAD.to_vec()),
            "permitted read call failed: {echo:?}"
        );

        let denied = plugin
            .call_call(&mut store, WRITE_OPERATION, PAYLOAD)
            .map_err(|error| anyhow::anyhow!("invoke denied guest call: {error}"))?;
        ensure!(
            denied == Err(bindings::tuilane::runtime_evaluation::host::HostError::Denied),
            "ungranted write was not denied: {denied:?}"
        );
        ensure!(counters.writes() == 0, "denied write reached its handler");

        let unknown = plugin
            .call_call(&mut store, "evaluation.unknown@1", &[])
            .map_err(|error| anyhow::anyhow!("invoke unknown operation: {error}"))?;
        ensure!(
            unknown
                == Err(bindings::tuilane::runtime_evaluation::host::HostError::UnknownOperation),
            "unknown operation was not rejected: {unknown:?}"
        );

        let initial_guest_memory = plugin
            .call_memory_size(&mut store)
            .map_err(|error| anyhow::anyhow!("read initial guest memory: {error}"))?;
        let allocation_succeeded = plugin
            .call_reserve_memory(&mut store, 1024 * 1024)
            .map_err(|error| anyhow::anyhow!("reserve bounded guest memory: {error}"))?;
        ensure!(allocation_succeeded, "small guest allocation failed");
        let after_small_allocation = plugin
            .call_memory_size(&mut store)
            .map_err(|error| anyhow::anyhow!("read guest memory after allocation: {error}"))?;
        ensure!(
            after_small_allocation <= MEMORY_LIMIT_BYTES as u64,
            "guest memory exceeded its configured store limit"
        );

        let oversized_request = u32::try_from(MEMORY_LIMIT_BYTES * 2)?;
        let oversized_allocation = plugin.call_reserve_memory(&mut store, oversized_request);
        ensure!(
            oversized_allocation.is_err() || matches!(oversized_allocation, Ok(false)),
            "guest allocation exceeded the configured memory limit"
        );
        let after_oversized_allocation = plugin
            .call_memory_size(&mut store)
            .map_err(|error| anyhow::anyhow!("recover after memory limit: {error}"))?;
        ensure!(
            after_oversized_allocation <= MEMORY_LIMIT_BYTES as u64,
            "guest memory exceeded its configured store limit"
        );

        let trap = plugin.call_trap(&mut store);
        ensure!(
            trap.is_err(),
            "guest trap unexpectedly returned successfully"
        );
        ensure!(
            plugin.call_metadata(&mut store).is_err(),
            "trapped component instance unexpectedly remained usable"
        );
        (
            counters,
            first_call,
            (
                initial_guest_memory,
                after_small_allocation,
                after_oversized_allocation,
            ),
        )
    };
    println!("First guest call: {}", format_duration(first_call));

    let (mut recovered_store, recovered_bindings) =
        instance(&engine, &component, EvaluationHost::new()?)?;
    recovered_bindings
        .tuilane_runtime_evaluation_plugin()
        .call_metadata(&mut recovered_store)
        .map_err(|error| {
            anyhow::anyhow!("recover after guest trap with a fresh instance: {error}")
        })?;

    let fuel_elapsed = verify_fuel_interruption(&engine, &component)?;

    let epoch_elapsed = verify_epoch_interruption(&engine, &component)?;
    let direct_samples = benchmark_direct_dispatch()?;
    let component_samples = benchmark_component_calls(&engine, &component)?;

    println!();
    println!("Runtime evaluation passed:");
    println!("- WIT host imports are permission-checked by tuilane-core.");
    println!(
        "- Read call succeeded; denied write handler count: {}.",
        counters.writes()
    );
    println!("- Unknown operation and oversized memory request were rejected.");
    println!(
        "- Traps, fuel exhaustion, epoch interruption, and fresh-instance recovery were verified."
    );
    println!(
        "- Guest linear memory: {} -> {} -> {} bytes (32 MiB limit).",
        guest_memory.0, guest_memory.1, guest_memory.2
    );
    println!(
        "- Fuel interruption: {}; epoch interruption: {}.",
        format_duration(fuel_elapsed),
        format_duration(epoch_elapsed)
    );
    println!("- Linux process RSS samples are reported only when /proc is available.");
    print_latency("Direct host dispatcher", &direct_samples);
    print_latency("Wasmtime component call", &component_samples);
    println!("Cold startup medians ({} samples):", startup.len());
    print_startup(
        "Engine creation",
        startup.iter().map(|sample| sample.engine).collect(),
    );
    print_startup(
        "Component load",
        startup.iter().map(|sample| sample.component).collect(),
    );
    print_startup(
        "Instance creation",
        startup.iter().map(|sample| sample.instance).collect(),
    );
    print_startup(
        "End-to-end",
        startup.iter().map(|sample| sample.total).collect(),
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct StartupSample {
    engine: Duration,
    component: Duration,
    instance: Duration,
    total: Duration,
}

fn measure_cold_startup(path: &Path) -> Result<Vec<StartupSample>> {
    let mut samples = Vec::with_capacity(STARTUP_SAMPLES);
    for _ in 0..STARTUP_SAMPLES {
        let total_started = Instant::now();
        let engine_started = Instant::now();
        let engine = engine()?;
        let engine_elapsed = engine_started.elapsed();

        let component_started = Instant::now();
        let component = Component::from_file(&engine, path)
            .map_err(|error| anyhow::anyhow!("compile component: {error}"))?;
        let component_elapsed = component_started.elapsed();

        let instance_started = Instant::now();
        let (store, _bindings) = instance(&engine, &component, EvaluationHost::new()?)?;
        let instance_elapsed = instance_started.elapsed();
        ensure!(store.get_fuel().is_ok(), "fuel accounting was not enabled");

        samples.push(StartupSample {
            engine: engine_elapsed,
            component: component_elapsed,
            instance: instance_elapsed,
            total: total_started.elapsed(),
        });
    }
    Ok(samples)
}

fn verify_fuel_interruption(engine: &wasmtime::Engine, component: &Component) -> Result<Duration> {
    let elapsed = {
        let (mut store, bindings) = instance(engine, component, EvaluationHost::new()?)?;
        store
            .set_fuel(10_000)
            .map_err(|error| anyhow::anyhow!("set low guest fuel limit: {error}"))?;
        let started = Instant::now();
        let result = bindings
            .tuilane_runtime_evaluation_plugin()
            .call_spin(&mut store);
        let elapsed = started.elapsed();
        let Err(error) = result else {
            anyhow::bail!("fuel limit did not stop the guest loop")
        };
        ensure!(
            format!("{error:?}").to_ascii_lowercase().contains("fuel"),
            "guest loop ended for an unexpected reason: {error:?}"
        );
        ensure!(
            bindings
                .tuilane_runtime_evaluation_plugin()
                .call_metadata(&mut store)
                .is_err(),
            "fuel-interrupted component instance unexpectedly remained usable"
        );
        elapsed
    };

    let (mut recovered_store, recovered_bindings) =
        instance(engine, component, EvaluationHost::new()?)?;
    recovered_bindings
        .tuilane_runtime_evaluation_plugin()
        .call_metadata(&mut recovered_store)
        .map_err(|error| {
            anyhow::anyhow!("recover after fuel exhaustion with a fresh instance: {error}")
        })?;
    Ok(elapsed)
}

fn verify_epoch_interruption(engine: &wasmtime::Engine, component: &Component) -> Result<Duration> {
    let elapsed = {
        let (mut store, bindings) = instance(engine, component, EvaluationHost::new()?)?;
        store.set_epoch_deadline(1);
        store
            .set_fuel(u64::MAX)
            .map_err(|error| anyhow::anyhow!("disable fuel limit for epoch test: {error}"))?;

        let interrupter = engine.clone();
        let interrupt = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            interrupter.increment_epoch();
        });

        let started = Instant::now();
        let result = bindings
            .tuilane_runtime_evaluation_plugin()
            .call_spin(&mut store);
        let elapsed = started.elapsed();
        interrupt
            .join()
            .map_err(|_| anyhow::anyhow!("epoch interrupter thread panicked"))?;
        let Err(error) = result else {
            anyhow::bail!("epoch deadline did not interrupt the guest loop")
        };
        ensure!(
            format!("{error:?}")
                .to_ascii_lowercase()
                .contains("interrupt"),
            "guest loop ended for an unexpected reason: {error:?}"
        );
        ensure!(
            bindings
                .tuilane_runtime_evaluation_plugin()
                .call_metadata(&mut store)
                .is_err(),
            "epoch-interrupted component instance unexpectedly remained usable"
        );
        elapsed
    };

    let (mut recovered_store, recovered_bindings) =
        instance(engine, component, EvaluationHost::new()?)?;
    recovered_bindings
        .tuilane_runtime_evaluation_plugin()
        .call_metadata(&mut recovered_store)
        .map_err(|error| {
            anyhow::anyhow!("recover after epoch interruption with a fresh instance: {error}")
        })?;
    Ok(elapsed)
}

fn benchmark_direct_dispatch() -> Result<Vec<u128>> {
    let mut host = EvaluationHost::new()?;
    for _ in 0..1_000 {
        host.dispatch(ECHO_OPERATION, PAYLOAD.to_vec())
            .map_err(|error| anyhow::anyhow!("direct warm-up call failed: {error:?}"))?;
    }

    let mut samples = Vec::with_capacity(LATENCY_SAMPLES);
    for _ in 0..LATENCY_SAMPLES {
        let started = Instant::now();
        host.dispatch(ECHO_OPERATION, PAYLOAD.to_vec())
            .map_err(|error| anyhow::anyhow!("direct benchmark call failed: {error:?}"))?;
        samples.push(started.elapsed().as_nanos());
    }
    Ok(samples)
}

fn benchmark_component_calls(
    engine: &wasmtime::Engine,
    component: &Component,
) -> Result<Vec<u128>> {
    let (mut store, bindings) = instance(engine, component, EvaluationHost::new()?)?;
    let plugin = bindings.tuilane_runtime_evaluation_plugin();
    for _ in 0..1_000 {
        let response = plugin
            .call_call(&mut store, ECHO_OPERATION, PAYLOAD)
            .map_err(|error| anyhow::anyhow!("component warm-up call trapped: {error}"))?;
        ensure!(response.is_ok(), "component warm-up call was denied");
    }

    let mut samples = Vec::with_capacity(LATENCY_SAMPLES);
    for _ in 0..LATENCY_SAMPLES {
        let started = Instant::now();
        let response = plugin
            .call_call(&mut store, ECHO_OPERATION, PAYLOAD)
            .map_err(|error| anyhow::anyhow!("component benchmark call trapped: {error}"))?;
        ensure!(response.is_ok(), "component benchmark call was denied");
        samples.push(started.elapsed().as_nanos());
    }
    Ok(samples)
}

fn print_latency(label: &str, samples: &[u128]) {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let median = sorted[(sorted.len() - 1) / 2];
    let p95 = sorted[((sorted.len() - 1) * 95) / 100];
    println!(
        "{label} latency ({} calls): p50={} ns, p95={} ns",
        samples.len(),
        median,
        p95
    );
}

fn print_startup(label: &str, samples: Vec<Duration>) {
    let mut nanos: Vec<_> = samples.iter().map(Duration::as_nanos).collect();
    nanos.sort_unstable();
    let median = nanos[(nanos.len() - 1) / 2];
    println!(
        "- {label}: {}",
        format_duration(Duration::from_nanos(median as u64))
    );
}

fn print_rss(stage: &str) {
    match current_rss_kib() {
        Some(kib) => println!("RSS ({stage}): {kib} KiB"),
        None => println!("RSS ({stage}): unavailable on this platform"),
    }
}

fn current_rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn format_duration(duration: Duration) -> String {
    if duration.as_millis() > 0 {
        format!("{:.2} ms", duration.as_secs_f64() * 1_000.0)
    } else {
        format!("{:.2} µs", duration.as_secs_f64() * 1_000_000.0)
    }
}
