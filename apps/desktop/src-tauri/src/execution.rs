use interl_core::{Diagnostic, ErrorCategory, ScenarioFormat};
#[cfg(test)]
use interl_engine::CancellationToken;
use interl_engine::{
    EnvironmentSecretProvider, PreparedRun, RunControl, RunEvent, RunHandle, RunOptions, RunReport,
    SecretProvider, StopCause,
};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Manager, State, ipc::Channel};
#[cfg(test)]
#[path = "../../../../tests/support/mod.rs"]
mod support;

#[derive(Default)]
pub struct RunRegistry(Mutex<Registry>);
#[derive(Default)]
struct Registry {
    active: Option<(String, RunControl)>,
    report: Option<RunReport>,
    next_reservation: u64,
}
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunMessage {
    Event {
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        event: Box<RunEvent>,
    },
    Finished {
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        report: Box<RunReport>,
    },
    Failed {
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        message: String,
    },
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedRun {
    run_id: String,
    protocol_version: u32,
}
fn diagnostic(message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCategory::Validation, "runOperation", "$", message)
}

pub fn prepare(
    source: &str,
    format: ScenarioFormat,
    options: RunOptions,
) -> Result<PreparedRun, Vec<Diagnostic>> {
    prepare_with_provider(source, format, options, &EnvironmentSecretProvider)
}
fn prepare_with_provider(
    source: &str,
    format: ScenarioFormat,
    options: RunOptions,
    provider: &dyn SecretProvider,
) -> Result<PreparedRun, Vec<Diagnostic>> {
    let document = interl_core::parse_document(source, format).map_err(|e| vec![e.diagnostic])?;
    PreparedRun::new_with_secret_provider(&document, options, provider)
}

// Never hold a registry lock while parsing or consulting a provider. The guard
// owns the preparing slot, cancels on failure, and only releases its own slot.
struct StartReservation<'a> {
    registry: &'a RunRegistry,
    id: String,
    cancel: RunControl,
    committed: bool,
}
impl Drop for StartReservation<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.cancel.cancel();
            if let Ok(mut registry) = self.registry.0.lock()
                && registry
                    .active
                    .as_ref()
                    .is_some_and(|(id, _)| id == &self.id)
            {
                registry.active = None;
            }
        }
    }
}
impl StartReservation<'_> {
    fn start(mut self, prepared: PreparedRun) -> Result<RunHandle, Vec<Diagnostic>> {
        let mut registry = self
            .registry
            .0
            .lock()
            .map_err(|_| vec![diagnostic("Run registry unavailable")])?;
        if self.cancel.is_cancelled()
            || !registry
                .active
                .as_ref()
                .is_some_and(|(id, _)| id == &self.id)
        {
            return Err(vec![diagnostic(
                "Run start was cancelled; no requests were sent",
            )]);
        }
        let handle = prepared.start_control(self.cancel.clone());
        registry.active = Some((handle.run_id.clone(), self.cancel.clone()));
        registry.report = None;
        self.committed = true;
        Ok(handle)
    }
}
impl RunRegistry {
    fn reserve(&self) -> Result<StartReservation<'_>, Vec<Diagnostic>> {
        let mut registry = self
            .0
            .lock()
            .map_err(|_| vec![diagnostic("Run registry unavailable")])?;
        if registry.active.is_some() {
            return Err(vec![diagnostic(
                "A run is already active; cancel it or wait for completion",
            )]);
        }
        registry.next_reservation = registry
            .next_reservation
            .checked_add(1)
            .ok_or_else(|| vec![diagnostic("Run registry capacity reached")])?;
        let id = format!("preparing:{}", registry.next_reservation);
        let cancel = RunControl::new();
        registry.active = Some((id.clone(), cancel.clone()));
        Ok(StartReservation {
            registry: self,
            id,
            cancel,
            committed: false,
        })
    }
    fn start(
        &self,
        prepare: impl FnOnce() -> Result<PreparedRun, Vec<Diagnostic>>,
    ) -> Result<RunHandle, Vec<Diagnostic>> {
        let reservation = self.reserve()?;
        if reservation.cancel.is_cancelled() {
            return Err(vec![diagnostic(
                "Run start was cancelled; no requests were sent",
            )]);
        }
        reservation.start(prepare()?)
    }
    fn finish(&self, id: &str, report: Option<&RunReport>) {
        if let Ok(mut registry) = self.0.lock()
            && registry
                .active
                .as_ref()
                .is_some_and(|(active, _)| active == id)
        {
            registry.active = None;
            registry.report = report.cloned();
        }
    }
    fn report_bytes(&self, id: &str) -> Result<Vec<u8>, Diagnostic> {
        let registry = self
            .0
            .lock()
            .map_err(|_| diagnostic("Run registry unavailable"))?;
        let report = registry
            .report
            .as_ref()
            .filter(|report| report.run_id == id)
            .ok_or_else(|| diagnostic("Completed report is no longer available"))?;
        serde_json::to_vec(report).map_err(|_| diagnostic("Report serialization failed"))
    }
    fn save_report(&self, path: &str, id: &str) -> Result<(), Diagnostic> {
        if !path.to_ascii_lowercase().ends_with(".json") {
            return Err(diagnostic("Choose a .json report file"));
        }
        let bytes = self.report_bytes(id)?;
        // File IO happens after releasing the registry lock.
        std::fs::write(path, bytes)
            .map_err(|_| diagnostic("Report could not be written; check file permissions"))
    }
    pub fn cancel(&self, id: &str) -> bool {
        let Ok(registry) = self.0.lock() else {
            return false;
        };
        if let Some((active, token)) = &registry.active
            && active == id
        {
            token.cancel();
            return true;
        }
        false
    }
    pub fn cancel_active(&self) {
        if let Ok(registry) = self.0.lock()
            && let Some((_, token)) = &registry.active
        {
            token.stop(StopCause::OwnerClosed);
        }
    }
}
// Shared forwarding path for native IPC and headless adapter tests. A failed
// delivery cancels the engine, but its safe partial report remains available.
async fn forward_events(
    mut handle: RunHandle,
    mut send: impl FnMut(RunEvent) -> bool,
) -> (String, Result<RunReport, tokio::task::JoinError>) {
    let mut ipc_dropped = 0u64;
    while let Some(event) = handle.events.recv().await {
        if !send(event) {
            ipc_dropped += 1;
            handle.control.stop(StopCause::ReceiverClosed);
        }
    }
    let result = handle.task.await.map(|mut report| {
        report.delivery_dropped_events = report.delivery_dropped_events.saturating_add(ipc_dropped);
        report
    });
    (handle.run_id, result)
}
#[tauri::command]
pub fn start_scenario(
    source: String,
    format: ScenarioFormat,
    options: Option<RunOptions>,
    channel: Channel<RunMessage>,
    state: State<'_, RunRegistry>,
    app: tauri::AppHandle,
) -> Result<StartedRun, Vec<Diagnostic>> {
    // The engine spawn needs Tauri's runtime; the reservation precedes all
    // preparation so rejected duplicate starts never perform secret lookup.
    let runtime = tauri::async_runtime::handle();
    let _entered = runtime.inner().enter();
    let handle = state.start(|| prepare(&source, format, options.unwrap_or_default()))?;
    let id = handle.run_id.clone();
    tauri::async_runtime::spawn(async move {
        let event_channel = channel.clone();
        let (id, result) = forward_events(handle, move |event| {
            event_channel
                .send(RunMessage::Event {
                    protocol_version: 3,
                    event: Box::new(event),
                })
                .is_ok()
        })
        .await;
        let state = app.state::<RunRegistry>();
        state.finish(&id, result.as_ref().ok());
        match result {
            Ok(report) => {
                let _ = channel.send(RunMessage::Finished {
                    protocol_version: 3,
                    report: Box::new(report),
                });
            }
            Err(_) => {
                let _ = channel.send(RunMessage::Failed {
                    protocol_version: 3,
                    message: "Execution task failed".into(),
                });
            }
        }
    });
    Ok(StartedRun {
        run_id: id,
        protocol_version: 3,
    })
}
#[tauri::command]
pub fn cancel_scenario(run_id: String, state: State<'_, RunRegistry>) -> bool {
    state.cancel(&run_id)
}
#[tauri::command]
pub fn save_run_report(
    path: String,
    run_id: String,
    state: State<'_, RunRegistry>,
) -> Result<(), Diagnostic> {
    state.save_report(&path, &run_id)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn desktop_preparation_and_headless_engine_have_equivalent_traces() {
        let server = support::TestServer::start().await;
        let json = include_str!("../../../../scenarios/examples/hello-api.json")
            .replace("http://127.0.0.1:8080", &server.base_url);
        let yaml = include_str!("../../../../scenarios/examples/hello-api.interl.yaml")
            .replace("http://127.0.0.1:8080", &server.base_url);
        let mut traces = Vec::new();
        for (source, format) in [(&json, ScenarioFormat::Json), (&yaml, ScenarioFormat::Yaml)] {
            let mut handle = prepare(source, format, RunOptions::default())
                .unwrap()
                .start(CancellationToken::new());
            while handle.events.recv().await.is_some() {}
            let report = handle.task.await.unwrap();
            assert_eq!(report.status, interl_engine::RunStatus::Passed);
            traces.push(
                report
                    .events
                    .into_iter()
                    .map(|e| {
                        let mut value = serde_json::to_value(e.event).unwrap();
                        if let Some(headers) = value
                            .pointer_mut("/payload/headers")
                            .and_then(serde_json::Value::as_object_mut)
                        {
                            headers.remove("date");
                        }
                        value
                    })
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(traces[0], traces[1]);
        let state = RunRegistry::default();
        assert!(!state.cancel("finished"));
    }
    #[test]
    fn only_active_run_can_be_cancelled() {
        let state = RunRegistry::default();
        let token = RunControl::new();
        state.0.lock().unwrap().active = Some(("active".into(), token.clone()));
        assert!(!state.cancel("finished"));
        assert!(!token.is_cancelled());
        assert!(state.cancel("active"));
        assert!(token.is_cancelled());
        state.0.lock().unwrap().active = None;
        assert!(!state.cancel("active"));
    }
}

#[cfg(test)]
#[path = "execution_auth_tests.rs"]
mod auth_tests;
