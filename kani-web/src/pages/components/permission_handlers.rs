use crate::{
    utils::use_permission_state,
    types::PermissionState,
};
use leptos::prelude::*;
use leptos_router::components::Redirect;

#[component]
pub fn PermissionGate(
    #[prop(optional)] permission: Option<&'static str>,
    children: ChildrenFn,
    #[prop(optional)] fallback: Option<ChildrenFn>,
    #[prop(optional)] loading: Option<ChildrenFn>,
) -> impl IntoView {
    let state = use_permission_state(permission);

    // Prepare the fallback view for the Suspense boundary
    let loading_view = move || {
        loading
            .as_ref()
            .map(|f| f().into_any())
            .unwrap_or_else(|| ().into_any())
    };

    view! {
        <Suspense fallback=loading_view>
            {move || match state.get() {
                PermissionState::Loading => ().into_any(), 
                PermissionState::Granted => children().into_any(),
                PermissionState::Denied  => fallback
                    .as_ref()
                    .map(|f| f().into_any())
                    .unwrap_or_else(|| ().into_any()),
            }}
        </Suspense>
    }
}

#[component]
pub fn RequirePermission(
    #[prop(optional)] permission: Option<&'static str>,
    children: ChildrenFn,
    #[prop(default = "/".to_string())] redirect_to: String,
) -> impl IntoView {
    let state = use_permission_state(permission);
    let redirect = StoredValue::new(redirect_to);

    view! {
        <Suspense fallback=|| ().into_any()>
            {move || match state.get() {
                PermissionState::Loading => ().into_any(),
                PermissionState::Granted => children().into_any(),
                PermissionState::Denied  => {
                    view! { <Redirect path=redirect.get_value() /> }.into_any()
                }
            }}
        </Suspense>
    }
}
