//! Source host management for WASM extensions.

use crate::error::{Error, Result};
use crate::wasm::HostState;
use std::time::{Duration, Instant};
use wasmtime::{Instance, Linker, Module, Store};

/// Hosts a single WASM source extension.
pub struct SourceHost {
    store: Store<HostState>,
    module: Module,
    instance: Option<Instance>,
    last_call: Option<Instant>,
}

impl SourceHost {
    /// Creates a new SourceHost from WASM binary.
    pub fn new(engine: &wasmtime::Engine, binary: &[u8]) -> Result<Self> {
        let module = Module::new(engine, binary)?;
        let store = Store::new(engine, HostState::default());

        Ok(Self {
            store,
            module,
            instance: None,
            last_call: None,
        })
    }

    /// Calls a function in the WASM module with the given arguments (async version).
    pub async fn call_function(
        &mut self,
        linker: &Linker<HostState>,
        function_name: &str,
        args: Vec<wasmtime::Val>,
    ) -> Result<Vec<wasmtime::Val>> {
        self.ensure_instantiated_async(linker).await?;
        let instance = self.instance.unwrap();

        let func = instance
            .get_func(&mut self.store, function_name)
            .ok_or_else(|| Error::Internal(format!("Function '{}' not found", function_name)))?;

        let func_ty = func.ty(&self.store);
        let result_count = func_ty.results().len();

        let mut results = vec![wasmtime::Val::I32(0); result_count];

        func.call_async(&mut self.store, &args, &mut results)
            .await
            .map_err(|e| Error::Internal(format!("WASM function call failed: {}", e)))?;

        Ok(results)
    }

    /// Calls a function that returns a string (ptr, len pair).
    pub async fn call_function_str(
        &mut self,
        linker: &Linker<HostState>,
        function_name: &str,
        args: Vec<wasmtime::Val>,
    ) -> Result<String> {
        let results = self.call_function(linker, function_name, args).await?;

        let ptr = results[0].unwrap_i32();
        let len = results[1].unwrap_i32();

        let string = self.read_memory_string(ptr, len)?;

        self.deallocate_memory(ptr, len)?;

        Ok(string)
    }

    /// Checks if the module should be unloaded due to inactivity.
    pub fn maybe_unload(&mut self) {
        if let Some(last_call) = self.last_call {
            if last_call.elapsed() > Duration::from_secs(60) {
                self.instance = None;
                self.last_call = None;
                self.store = Store::new(self.store.engine(), HostState::default());
            }
        }
    }

    /// Returns whether the module is currently loaded.
    pub fn is_loaded(&self) -> bool {
        self.instance.is_some()
    }

    async fn ensure_instantiated_async(&mut self, linker: &Linker<HostState>) -> Result<&Instance> {
        self.last_call = Some(Instant::now());

        if self.instance.is_none() {
            let instance = linker
                .instantiate_async(&mut self.store, &self.module)
                .await?;
            self.instance = Some(instance);
        }

        match self.instance.as_ref() {
            Some(instance) => Ok(instance),
            None => Err(Error::Internal("Instance not found".to_string())),
        }
    }

    fn read_memory_string(&mut self, ptr: i32, len: i32) -> Result<String> {
        let memory = self
            .instance
            .unwrap()
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| Error::WasmMemoryAccess("memory export not found".to_string()))?;
        let data = memory.data(&self.store);
        let bytes = &data[ptr as usize..ptr as usize + len as usize];
        String::from_utf8(bytes.to_vec()).map_err(|e| Error::WasmMemoryAccess(e.to_string()))
    }

    fn deallocate_memory(&mut self, ptr: i32, len: i32) -> Result<()> {
        let instance = self
            .instance
            .ok_or_else(|| Error::Internal("Instance not available".to_string()))?;

        let dealloc_func = instance
            .get_func(&mut self.store, "deallocate")
            .or_else(|| instance.get_func(&mut self.store, "free"))
            .or_else(|| instance.get_func(&mut self.store, "__free"));

        if let Some(func) = dealloc_func {
            let mut results = vec![];
            func.call(
                &mut self.store,
                &[wasmtime::Val::I32(ptr), wasmtime::Val::I32(len)],
                &mut results,
            )
            .map_err(|e| Error::Internal(format!("Deallocation failed: {}", e)))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // SourceHost tests would require actual WASM modules
    // These are integration-level tests
}
