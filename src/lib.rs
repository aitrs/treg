pub mod registry;
pub mod task;

pub use registry::TaskDescriptor;
pub use task::{Routine, Task};

pub async fn list_all() -> Vec<TaskDescriptor> {
    registry::list_all().await
}

pub async fn count() -> usize {
    registry::count().await
}
