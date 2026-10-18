use crate::core::Progress;

pub fn sink<T: Send + Sync + 'static>(value: impl Fn(T) + Send + Sync + 'static) -> Progress<T> {
    std::sync::Arc::new(value)
}
