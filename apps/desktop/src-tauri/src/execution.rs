use interl_core::{Diagnostic, ErrorCategory, ScenarioFormat};
use interl_engine::{CancellationToken, PreparedRun, RunEvent, RunOptions, RunReport};
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
    active: Option<(String, CancellationToken)>,
    report: Option<RunReport>,
}
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunMessage {
    Event { event: RunEvent },
    Finished { report: Box<RunReport> },
    Failed { message: String },
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedRun {
    run_id: String,
}
fn diagnostic(message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCategory::Validation, "runOperation", "$", message)
}

pub fn prepare(
    source: &str,
    format: ScenarioFormat,
    options: RunOptions,
) -> Result<PreparedRun, Vec<Diagnostic>> {
    let document = interl_core::parse_scenario(source, format).map_err(|e| vec![e.diagnostic])?;
    PreparedRun::new(&document, options)
}
impl RunRegistry {
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
            token.cancel();
        }
    }
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
    let prepared = prepare(&source, format, options.unwrap_or_default())?;
    let mut registry = state
        .0
        .lock()
        .map_err(|_| vec![diagnostic("Run registry unavailable")])?;
    if registry.active.is_some() {
        return Err(vec![diagnostic(
            "A run is already active; cancel it or wait for completion",
        )]);
    }
    let cancel = CancellationToken::new();
    // Command is synchronous, so enter Tauri's Tokio runtime before spawning the engine.
    let runtime = tauri::async_runtime::handle();
    let _entered = runtime.inner().enter();
    let mut handle = prepared.start(cancel.clone());
    let id = handle.run_id.clone();
    registry.active = Some((id.clone(), cancel.clone()));
    registry.report = None;
    drop(registry);
    tauri::async_runtime::spawn(async move {
        while let Some(event) = handle.events.recv().await {
            if channel.send(RunMessage::Event { event }).is_err() {
                cancel.cancel();
            }
        }
        let result = handle.task.await;
        let state = app.state::<RunRegistry>();
        if let Ok(mut registry) = state.0.lock()
            && registry
                .active
                .as_ref()
                .is_some_and(|(active, _)| active == &handle.run_id)
        {
            registry.active = None;
            if let Ok(report) = &result {
                registry.report = Some(report.clone());
            }
        }
        match result {
            Ok(report) => {
                let _ = channel.send(RunMessage::Finished {
                    report: Box::new(report),
                });
            }
            Err(_) => {
                let _ = channel.send(RunMessage::Failed {
                    message: "Execution task failed".into(),
                });
            }
        }
    });
    Ok(StartedRun { run_id: id })
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
    if !path.to_ascii_lowercase().ends_with(".json") {
        return Err(diagnostic("Choose a .json report file"));
    }
    let registry = state
        .0
        .lock()
        .map_err(|_| diagnostic("Run registry unavailable"))?;
    let report = registry
        .report
        .as_ref()
        .filter(|r| r.run_id == run_id)
        .ok_or_else(|| diagnostic("Completed report is no longer available"))?;
    let bytes =
        serde_json::to_vec(report).map_err(|_| diagnostic("Report serialization failed"))?;
    std::fs::write(path, bytes)
        .map_err(|_| diagnostic("Report could not be written; check file permissions"))
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
        let token = CancellationToken::new();
        state.0.lock().unwrap().active = Some(("active".into(), token.clone()));
        assert!(!state.cancel("finished"));
        assert!(!token.is_cancelled());
        assert!(state.cancel("active"));
        assert!(token.is_cancelled());
        state.0.lock().unwrap().active = None;
        assert!(!state.cancel("active"));
    }
}
