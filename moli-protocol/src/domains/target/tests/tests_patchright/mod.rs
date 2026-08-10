use super::*;

pub(super) async fn patchright_8mb_stack<F, Fut>(name: &'static str, build: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()>,
{
    let result = std::thread::Builder::new()
        .name(name.into())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("8mb-stack patchright test runtime should build")
                .block_on(build());
        })
        .expect("8mb-stack patchright test thread should spawn")
        .join();

    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

mod binding_isolation;
mod crpage_cleanup;
mod crpage_expose;
mod handle_wrapper;
mod utility_world;
