pub mod registry;
pub mod task;

pub use registry::TaskDescriptor;
pub use task::{Routine, Task};
use anyhow::Result;

pub async fn list_all() -> Vec<TaskDescriptor> {
    registry::list_all().await
}

pub async fn count() -> usize {
    registry::count().await
}

pub async fn cancel(id: u64) -> Result<()> {
    registry::cancel(id).await
}
