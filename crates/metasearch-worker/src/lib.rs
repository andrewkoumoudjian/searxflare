#[cfg(target_arch = "wasm32")]
mod runtime;

#[cfg(target_arch = "wasm32")]
use worker::*;

#[cfg(target_arch = "wasm32")]
#[event(fetch)]
pub async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    runtime::handle(req, env, ctx).await
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_test_marker() -> &'static str {
    "metasearch-worker compiles natively for tests; production runs on wasm32-unknown-unknown"
}
