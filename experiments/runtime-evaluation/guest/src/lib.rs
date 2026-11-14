wit_bindgen::generate!({
    path: "../wit",
    world: "evaluation",
});

struct EvaluationPlugin;

impl exports::tuilane::runtime_evaluation::plugin::Guest for EvaluationPlugin {
    fn metadata() -> exports::tuilane::runtime_evaluation::plugin::PluginMetadata {
        exports::tuilane::runtime_evaluation::plugin::PluginMetadata {
            id: "evaluation.echo".to_owned(),
            version: "0.1.0".to_owned(),
        }
    }

    fn call(
        operation: String,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, tuilane::runtime_evaluation::host::HostError> {
        tuilane::runtime_evaluation::host::dispatch(&operation, &payload)
    }

    fn reserve_memory(bytes: u32) -> bool {
        let mut allocation = Vec::<u8>::new();
        if allocation.try_reserve_exact(bytes as usize).is_err() {
            return false;
        }

        allocation.resize(bytes as usize, 1);
        std::hint::black_box(allocation);
        true
    }

    fn memory_size() -> u64 {
        (core::arch::wasm32::memory_size(0) * 65_536) as u64
    }

    fn spin() {
        loop {
            std::hint::spin_loop();
        }
    }

    fn trap() {
        core::arch::wasm32::unreachable();
    }
}

export!(EvaluationPlugin);
