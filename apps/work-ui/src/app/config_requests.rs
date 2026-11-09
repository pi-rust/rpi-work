//! Deadlines for short configuration RPCs only, never for generation or WebSocket streams.
//! Dropping a timed-out future stops waiting locally, not the remote operation.
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::task::Poll;
use work_client::{ClientError, RuntimeState};

/// RPI catalog affordances come from the bound session, never global protocol
/// capabilities or an optional native handle. ACP uses opaque configOptions.
#[allow(clippy::struct_excessive_bools)] // Four independent observed commands, not mutually exclusive states.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct CatalogAccess {
    pub models_read: bool,
    pub models_write: bool,
    pub thinking_read: bool,
    pub thinking_write: bool,
}
impl CatalogAccess {
    pub(super) fn for_configuration(config: &work_client::SessionConfiguration) -> Self {
        if config.protocol != "rpi" {
            return Self::default();
        }
        Self {
            models_read: config.supports_command("get_available_models"),
            models_write: config.supports_command("set_model"),
            thinking_read: config.supports_command("get_available_thinking_levels"),
            thinking_write: config.supports_command("set_thinking_level"),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ConfigField {
    Model,
    Thinking,
}

impl ConfigField {
    pub(super) fn reported_by(self, state: &RuntimeState) -> bool {
        let value = match self {
            Self::Model => state.model_id.as_deref(),
            Self::Thinking => state.thinking_level.as_deref(),
        };
        value.is_some_and(|value| !value.trim().is_empty())
    }
}

pub(super) async fn bounded<T>(
    operation: &'static str,
    future: impl Future<Output = Result<T, ClientError>>,
) -> Result<T, ClientError> {
    race(operation, future, async {
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::TimeoutFuture::new(15_000).await;
        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    })
    .await
}

async fn race<T>(
    operation: &'static str,
    future: impl Future<Output = Result<T, ClientError>>,
    deadline: impl Future<Output = ()>,
) -> Result<T, ClientError> {
    let mut future = pin!(future);
    let mut deadline = pin!(deadline);
    poll_fn(move |cx| {
        if let Poll::Ready(result) = future.as_mut().poll(cx) {
            return Poll::Ready(result);
        }
        if deadline.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(ClientError::RequestTimeout(format!(
                "{operation}超时（15 秒）；超时不代表节点操作已取消，请刷新核对后重试"
            ))));
        }
        Poll::Pending
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::future::{pending, ready};
    use std::rc::Rc;
    use std::task::{Context, Waker};

    #[test]
    fn catalog_access_uses_exact_session_commands_not_global_defaults() {
        let config = work_client::SessionConfiguration {
            protocol: "rpi".into(),
            supported_commands: vec![
                "get_available_models".into(),
                "set_thinking_level".into(),
                "set_model_extra".into(),
            ],
            config_options: vec![],
        };
        let access = CatalogAccess::for_configuration(&config);
        assert!(access.models_read);
        assert!(!access.models_write);
        assert!(!access.thinking_read);
        assert!(access.thinking_write);
        assert_eq!(
            CatalogAccess::for_configuration(&work_client::SessionConfiguration {
                supported_commands: vec![],
                ..config
            }),
            CatalogAccess::default()
        );
    }

    #[test]
    fn acp_and_unknown_protocols_never_get_rpi_catalog_aliases() {
        for protocol in ["acp", "", "future"] {
            let config = work_client::SessionConfiguration {
                protocol: protocol.into(),
                supported_commands: vec![
                    "get_available_models".into(),
                    "set_model".into(),
                    "get_available_thinking_levels".into(),
                    "set_thinking_level".into(),
                ],
                config_options: vec![serde_json::json!({"id":"opaque-model-option"})],
            };
            assert_eq!(
                CatalogAccess::for_configuration(&config),
                CatalogAccess::default()
            );
        }
    }

    fn poll_ready<T>(future: impl Future<Output = T>) -> T {
        let mut future = pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("expected a ready result"),
        }
    }

    #[test]
    fn ready_results_and_errors_are_not_replaced_by_timeout() {
        assert_eq!(
            poll_ready(race("test", ready(Ok(42)), ready(()))).unwrap(),
            42
        );
        assert!(matches!(
            poll_ready(race::<()>("test", ready(Err(ClientError::Protocol("reject".into()))), pending())),
            Err(ClientError::Protocol(message)) if message == "reject"
        ));
    }

    #[test]
    fn timeout_drops_pending_work_instead_of_committing_a_late_result() {
        struct Dropped(Rc<Cell<bool>>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        let dropped = Rc::new(Cell::new(false));
        let guard = Dropped(dropped.clone());
        let work = async move {
            let _guard = guard;
            pending::<()>().await;
            Ok::<_, ClientError>(42)
        };
        assert!(matches!(
            poll_ready(race("读取配置", work, ready(()))),
            Err(ClientError::RequestTimeout(_))
        ));
        assert!(dropped.get());
    }

    #[test]
    fn verification_requires_the_affected_field_not_an_empty_state() {
        let mut state: RuntimeState = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!ConfigField::Model.reported_by(&state));
        assert!(!ConfigField::Thinking.reported_by(&state));
        state.model_id = Some("raw/model".into());
        state.thinking_level = Some(" ".into());
        assert!(ConfigField::Model.reported_by(&state));
        assert!(!ConfigField::Thinking.reported_by(&state));
        state.thinking_level = Some("off".into());
        assert!(ConfigField::Thinking.reported_by(&state));
    }
}
