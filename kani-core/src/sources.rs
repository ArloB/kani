//! Source host management for WASM extensions.

use crate::error::{Error, Result};
use crate::wasm::HostState;
use std::time::{Duration, Instant};
use wasmtime::{Instance, Linker, Module, Store};

/// Hosts a single WASM source extension.
pub struct SourceHost {
    store: Option<Store<HostState>>,
    module: Option<Module>,
    instance: Option<Instance>,
    last_call: Option<Instant>,
    solver_url: Option<String>,
    source_name: String,
}

impl SourceHost {
    pub fn new(solver_url: Option<String>, source_name: &str) -> Self {
        Self {
            store: None,
            module: None,
            instance: None,
            last_call: None,
            solver_url,
            source_name: source_name.to_string(),
        }
    }

    /// Calls a function in the WASM module with the given arguments (async version).
    pub async fn call_function(
        &mut self,
        linker: &Linker<HostState>,
        function_name: &str,
        args: Vec<wasmtime::Val>,
    ) -> Result<Vec<wasmtime::Val>> {
        self.ensure_instantiated_async(linker).await?;
        let store = self
            .store
            .as_mut()
            .ok_or_else(|| Error::Internal("Store not initialized".to_string()))?;
        let instance = self
            .instance
            .ok_or_else(|| Error::Internal("Instance not loaded".to_string()))?;

        let func = instance
            .get_func(&mut *store, function_name)
            .ok_or_else(|| Error::Internal(format!("Function '{}' not found", function_name)))?;

        let func_ty = func.ty(&mut *store);
        let result_count = func_ty.results().len();

        let mut results = vec![wasmtime::Val::I32(0); result_count];

        func.call_async(&mut *store, &args, &mut results)
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

        let (ptr, len) = if results.len() == 1 {
            // Packed u64 (ptr << 32 | len)
            let packed = results[0]
                .i64()
                .ok_or_else(|| Error::Internal("Expected i64 return value".to_string()))?
                as u64;
            let len = (packed & 0xFFFFFFFF) as i32;
            let ptr = (packed >> 32) as i32;
            (ptr, len)
        } else if results.len() == 2 {
            // Two i32s (legacy/standard multi-value)
            let ptr = results[0]
                .i32()
                .ok_or_else(|| Error::Internal("Expected i32 return value for ptr".to_string()))?;
            let len = results[1]
                .i32()
                .ok_or_else(|| Error::Internal("Expected i32 return value for len".to_string()))?;
            (ptr, len)
        } else {
            return Err(Error::Internal(format!(
                "Expected 1 or 2 return values for string function, got {}",
                results.len()
            )));
        };

        let string = self.read_memory_string(ptr, len)?;

        self.deallocate_memory(ptr, len).await?;

        Ok(string)
    }

    /// Checks if the module should be unloaded due to inactivity.
    pub fn maybe_unload(&mut self) -> Result<()> {
        if self
            .last_call
            .is_some_and(|last_call| last_call.elapsed() > Duration::from_secs(60))
        {
            self.instance = None;
            self.last_call = None;
            if let Some(store) = self.store.as_ref() {
                let engine = store.engine();
                if let Ok(state) = HostState::new(self.solver_url.clone()) {
                    self.store = Some(Store::new(engine, state));
                } else {
                    self.store = Some(Store::new(engine, HostState::new(self.solver_url.clone())?));
                }
            }
        }
        Ok(())
    }

    /// Loads a source from the file system.
    pub async fn load(
        mut self,
        engine: &wasmtime::Engine,
        wasm_storage_path: &std::path::Path,
    ) -> Result<Self> {
        let wasm_path = wasm_storage_path.join(format!("{}.wasm", self.source_name));
        tracing::info!(
            "Loading source: {} ({})",
            self.source_name,
            wasm_path.display()
        );

        let bytes = tokio::fs::read(&wasm_path).await.map_err(Error::Io)?;

        self.module = Some(Module::new(engine, &bytes)?);
        self.store = Some(Store::new(engine, HostState::new(self.solver_url.clone())?));
        self.instance = None;

        Ok(self)
    }

    /// Returns whether the module is currently loaded.
    pub fn is_loaded(&self) -> bool {
        self.instance.is_some()
    }

    async fn ensure_instantiated_async(&mut self, linker: &Linker<HostState>) -> Result<&Instance> {
        self.last_call = Some(Instant::now());

        if self.instance.is_none() {
            let store = self
                .store
                .as_mut()
                .ok_or_else(|| Error::Internal("Store not initialized".to_string()))?;
            let module = self
                .module
                .as_ref()
                .ok_or_else(|| Error::Internal("Module not loaded".to_string()))?;
            let instance = linker.instantiate_async(store, module).await?;
            self.instance = Some(instance);
        }

        match self.instance.as_ref() {
            Some(instance) => Ok(instance),
            None => Err(Error::Internal("Instance not found".to_string())),
        }
    }

    fn read_memory_string(&mut self, ptr: i32, len: i32) -> Result<String> {
        let store = self
            .store
            .as_mut()
            .ok_or_else(|| Error::Internal("Store not initialized".to_string()))?;
        let memory = self
            .instance
            .ok_or_else(|| Error::Internal("Instance not loaded".to_string()))?
            .get_memory(&mut *store, "memory")
            .ok_or_else(|| Error::WasmMemoryAccess("memory export not found".to_string()))?;
        let data = memory.data(&*store);
        let bytes = &data[ptr as usize..ptr as usize + len as usize];
        String::from_utf8(bytes.to_vec()).map_err(|e| Error::WasmMemoryAccess(e.to_string()))
    }

    pub async fn deallocate_memory(&mut self, ptr: i32, len: i32) -> Result<()> {
        let store = self
            .store
            .as_mut()
            .ok_or_else(|| Error::Internal("Store not initialized".to_string()))?;
        let instance = self
            .instance
            .ok_or_else(|| Error::Internal("Instance not available".to_string()))?;

        let mut dealloc_func = instance.get_func(&mut *store, "deallocate");
        if dealloc_func.is_none() {
            dealloc_func = instance.get_func(&mut *store, "free");
        }
        if dealloc_func.is_none() {
            dealloc_func = instance.get_func(&mut *store, "__free");
        }

        if let Some(func) = dealloc_func {
            let mut results = vec![];
            func.call_async(
                &mut *store,
                &[wasmtime::Val::I32(ptr), wasmtime::Val::I32(len)],
                &mut results,
            )
            .await
            .map_err(|e| Error::Internal(format!("Deallocation failed: {}", e)))?;
        }

        Ok(())
    }

    pub async fn write_string(
        &mut self,
        linker: &Linker<HostState>,
        s: &str,
    ) -> Result<(i32, i32)> {
        self.ensure_instantiated_async(linker).await?;

        let bytes = s.as_bytes();
        let len = bytes.len() as i32;

        let store = self
            .store
            .as_mut()
            .ok_or_else(|| Error::Internal("Store not initialized".to_string()))?;

        let instance = self
            .instance
            .as_ref()
            .ok_or_else(|| Error::Internal("Instance not available for allocation".to_string()))?;

        let mut alloc_func = instance.get_func(&mut *store, "allocate");
        if alloc_func.is_none() {
            alloc_func = instance.get_func(&mut *store, "malloc");
        }
        let alloc_func = alloc_func
            .ok_or_else(|| Error::Internal("Allocation function not found".to_string()))?;

        let mut results = vec![wasmtime::Val::I32(0)];
        alloc_func
            .call_async(&mut *store, &[wasmtime::Val::I32(len)], &mut results)
            .await
            .map_err(|e| Error::Internal(format!("Allocation failed: {}", e)))?;

        let ptr = results[0]
            .i32()
            .ok_or_else(|| Error::Internal("Expected i32 return value from alloc".to_string()))?;

        let memory = self
            .instance
            .unwrap()
            .get_memory(&mut *store, "memory")
            .ok_or_else(|| Error::WasmMemoryAccess("memory export not found".to_string()))?;

        memory
            .write(&mut *store, ptr as usize, bytes)
            .map_err(|e| {
                Error::WasmMemoryAccess(format!("Failed to write string to memory: {}", e))
            })?;

        Ok((ptr, len))
    }
}

#[cfg(test)]
mod tests {
    // SourceHost tests would require actual WASM modules
    // These are integration-level tests
}
