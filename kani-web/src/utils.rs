//! Utilities used across both SSR and hydrate builds.

use leptos::prelude::*;

/// Sleep for `ms` milliseconds in a way that works in both WASM (browser) and
/// native (SSR) builds.
pub async fn sleep_ms(ms: u32) {
    #[cfg(feature = "hydrate")]
    gloo_timers::future::TimeoutFuture::new(ms).await;

    #[cfg(feature = "ssr")]
    tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;

    #[cfg(not(any(feature = "hydrate", feature = "ssr")))]
    let _ = ms;
}

/// Returns a debounced version of `source` that only updates after `delay_ms`
/// of inactivity.
pub fn use_debounced_signal<T>(source: ReadSignal<T>, delay_ms: u32) -> ReadSignal<T>
where
    T: Clone + Send + Sync + 'static + PartialEq,
{
    let (debounced, set_debounced) = signal(source.get_untracked());

    Effect::new(move |_| {
        let val = source.get();
        leptos::task::spawn_local(async move {
            sleep_ms(delay_ms).await;
            if source.get_untracked() == val {
                set_debounced.set(val);
            }
        });
    });

    debounced
}

/// Persist a boolean flag to localStorage under `key`.
/// No-op in SSR builds.
pub fn set_local_flag(key: &str, value: bool) {
    #[cfg(feature = "hydrate")]
    {
        use leptos::web_sys::window;
        if let Some(storage) = window().and_then(|w| w.local_storage().ok()).flatten() {
            if value {
                let _ = storage.set_item(key, "1");
            } else {
                let _ = storage.remove_item(key);
            }
        }
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = (key, value);
}

/// Read a boolean flag from localStorage.
/// Always returns `false` in SSR builds.
pub fn get_local_flag(key: &str) -> bool {
    #[cfg(feature = "hydrate")]
    {
        use leptos::web_sys::window;
        window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
            .and_then(|s| s.get_item(key).ok())
            .flatten()
            .as_deref() == Some("1")
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = key;
        false
    }
}