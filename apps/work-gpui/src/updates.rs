use std::{future::Future, sync::Arc, time::Duration};

use gpui::{App, Global};
use tokio::{runtime::Runtime, sync::mpsc, task::AbortHandle};
use work_update::{Release, native::gpui as installer};

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");
const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub(crate) enum Status {
    Idle,
    Checking,
    Current,
    Available(Release),
    Downloading {
        version: String,
        percent: u64,
    },
    Ready {
        version: String,
        update: Arc<installer::Prepared>,
    },
    Installing,
    Failed(String),
}

pub(crate) struct Updates {
    pub enabled: bool,
    pub status: Status,
    pub install_problem: Option<String>,
    runtime: Option<Runtime>,
    automatic: bool,
    started: bool,
    active: Option<Job>,
    serial: u64,
    arguments: Vec<String>,
    local_owner: Option<Arc<work_host::setup::DesktopSetup>>,
}

struct Job {
    serial: u64,
    automatic: bool,
    abort: AbortHandle,
}

enum Event {
    Downloading { version: String, percent: u64 },
    Finished(Result<Status, String>),
}

impl Global for Updates {}

impl Updates {
    pub fn initialize(arguments: Vec<String>, cx: &mut App) {
        let enabled = option_env!("RPI_WORK_GPUI_DISTRIBUTION") == Some("1")
            && !arguments.iter().any(|argument| argument == "--sample")
            && installer::supports_install();
        let (runtime, problem) = if enabled {
            match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(1)
                .build()
            {
                Ok(runtime) => (Some(runtime), None),
                Err(error) => (None, Some(error.to_string())),
            }
        } else {
            (None, None)
        };
        let previous_error = std::env::var(installer::RESULT_ENV).ok();
        let status = previous_error
            .map(|error| format!("安装失败：{error}"))
            .or(problem)
            .map_or(Status::Idle, Status::Failed);
        cx.set_global(Self {
            enabled: runtime.is_some(),
            status,
            install_problem: None,
            runtime,
            automatic: true,
            started: false,
            active: None,
            serial: 0,
            arguments,
            local_owner: None,
        });
        let cleanup_delay = cx.background_executor().timer(Duration::from_secs(1));
        cx.background_executor()
            .spawn(async {
                cleanup_delay.await;
                installer::cleanup_previous_helper();
            })
            .detach();
    }

    pub fn set_local_owner(owner: Arc<work_host::setup::DesktopSetup>, cx: &mut App) {
        cx.global_mut::<Self>().local_owner = Some(owner);
    }

    pub fn start(automatic: bool, cx: &mut App) {
        let state = cx.global_mut::<Self>();
        if !state.enabled || state.started {
            return;
        }
        state.started = true;
        state.automatic = automatic;
        // Keep a failed restart visible until the next manual check or interval.
        if automatic && matches!(state.status, Status::Idle) {
            Self::check(true, cx);
        }
        let clock = cx.background_executor().clone();
        cx.spawn(async move |cx| {
            loop {
                clock.timer(INTERVAL).await;
                if cx.update(|cx| Self::check(true, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    pub fn set_automatic(enabled: bool, cx: &mut App) {
        let state = cx.global_mut::<Self>();
        if !state.started || !state.enabled || state.automatic == enabled {
            return;
        }
        state.automatic = enabled;
        if enabled {
            Self::check(true, cx);
        } else if state.active.as_ref().is_some_and(|job| job.automatic)
            && let Some(job) = state.active.take()
        {
            job.abort.abort();
            state.status = Status::Idle;
        }
    }

    pub fn busy(&self) -> bool {
        matches!(
            self.status,
            Status::Checking | Status::Downloading { .. } | Status::Installing
        )
    }

    pub fn check(automatic: bool, cx: &mut App) {
        let state = cx.global::<Self>();
        if !state.enabled
            || state.busy()
            || matches!(state.status, Status::Ready { .. })
            || automatic && !state.automatic
        {
            return;
        }
        let download = state.automatic;
        Self::run(Status::Checking, automatic, cx, move |events| async move {
            match installer::check(VERSION)
                .await
                .map_err(|error| format!("检查更新失败：{error}"))?
            {
                Some(release) if download => Self::fetch(release, &events).await,
                Some(release) => Ok(Status::Available(release)),
                None => Ok(Status::Current),
            }
        });
    }

    pub fn download(cx: &mut App) {
        let Status::Available(release) = &cx.global::<Self>().status else {
            return;
        };
        let release = release.clone();
        Self::run(
            Status::Downloading {
                version: release.manifest.version.clone(),
                percent: 0,
            },
            false,
            cx,
            move |events| async move { Self::fetch(release, &events).await },
        );
    }

    async fn fetch(
        release: Release,
        events: &mpsc::UnboundedSender<Event>,
    ) -> Result<Status, String> {
        let version = release.manifest.version.clone();
        let mut percent = 0;
        let _ = events.send(Event::Downloading {
            version: version.clone(),
            percent,
        });
        let update = installer::download(release, |received, total| {
            let next = received.saturating_mul(100) / total.max(1);
            if next != percent {
                percent = next;
                let _ = events.send(Event::Downloading {
                    version: version.clone(),
                    percent,
                });
            }
        })
        .await
        .map_err(|error| format!("下载更新失败：{error}"))?;
        Ok(Status::Ready {
            version,
            update: Arc::new(update),
        })
    }

    fn run<F>(
        initial: Status,
        automatic: bool,
        cx: &mut App,
        run: impl FnOnce(mpsc::UnboundedSender<Event>) -> F,
    ) where
        F: Future<Output = Result<Status, String>> + Send + 'static,
    {
        let state = cx.global_mut::<Self>();
        let Some(runtime) = &state.runtime else {
            return;
        };
        let (send, mut events) = mpsc::unbounded_channel();
        let operation = run(send.clone());
        let task = runtime.spawn(async move {
            let result = operation.await;
            let _ = send.send(Event::Finished(result));
        });
        state.serial += 1;
        let serial = state.serial;
        state.status = initial;
        state.active = Some(Job {
            serial,
            automatic,
            abort: task.abort_handle(),
        });
        cx.spawn(async move |cx| {
            while let Some(event) = events.recv().await {
                if cx
                    .update(|cx| {
                        let state = cx.global_mut::<Self>();
                        if state.active.as_ref().is_none_or(|job| job.serial != serial) {
                            return;
                        }
                        match event {
                            Event::Downloading { version, percent } => {
                                state.status = Status::Downloading { version, percent }
                            }
                            Event::Finished(result) => {
                                state.active = None;
                                state.status = result.unwrap_or_else(Status::Failed);
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
            let _ = cx.update(|cx| {
                let state = cx.global_mut::<Self>();
                if state
                    .active
                    .as_ref()
                    .is_some_and(|job| job.serial == serial)
                {
                    state.active = None;
                    state.status = Status::Failed("更新任务意外结束，请重试。".into());
                }
            });
        })
        .detach();
    }

    pub fn install(cx: &mut App) {
        let state = cx.global_mut::<Self>();
        let Status::Ready { version, update } = &state.status else {
            return;
        };
        let update = update.clone();
        let version = version.clone();
        let arguments = state.arguments.clone();
        let owner = state.local_owner.clone();
        state.status = Status::Installing;
        state.install_problem = None;
        let install = cx.background_executor().spawn(async move {
            let result = installer::start_install(&update, arguments);
            if result.is_ok()
                && let Some(owner) = owner
            {
                owner.shutdown();
            }
            (result, update)
        });
        cx.spawn(async move |cx| {
            let (result, update) = install.await;
            let _ = cx.update(|cx| match result {
                Ok(()) => {
                    drop(update);
                    cx.quit();
                }
                Err(error) => {
                    let state = cx.global_mut::<Self>();
                    state.status = Status::Ready { version, update };
                    // Keep the verified package for another attempt.
                    state.install_problem = Some(error);
                }
            });
        })
        .detach();
    }
}
