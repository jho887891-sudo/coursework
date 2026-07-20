use lesson_03_tools::*;
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let scenario = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "transient".to_owned());
    match scenario.as_str() {
        "transient" => transient_recovery().await,
        "permission" => permission_denied().await,
        "lost-response" => lost_response().await,
        other => {
            eprintln!("未知场景：{other}；可选 transient、permission、lost-response");
            std::process::exit(2);
        }
    }
}

fn request(id: &str, name: &str, arguments: serde_json::Value, key: Option<&str>) -> ToolRequest {
    ToolRequest {
        request_id: id.to_owned(),
        name: name.to_owned(),
        arguments,
        idempotency_key: key.map(str::to_owned),
    }
}

fn print_trace(registry: &Registry) {
    for event in registry.trace() {
        println!(
            "{}",
            serde_json::to_string(event).expect("TraceEvent must serialize")
        );
    }
}

async fn transient_recovery() {
    let mut registry = Registry::default();
    registry
        .register(Box::new(FaultyTool::new(
            [FaultMode::Transient, FaultMode::Success],
            100,
        )))
        .unwrap();
    let observation = registry
        .execute(
            &request("demo-transient", "faulty", json!({}), None),
            &ExecutionPolicy::read_only(1),
        )
        .await;
    print_trace(&registry);
    eprintln!("OBSERVATION={observation:?}");
}

async fn permission_denied() {
    let tool = CounterTool::new(100);
    let handle = tool.handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let observation = registry
        .execute(
            &request(
                "demo-permission",
                "counter_add",
                json!({"key":"score","amount":1}),
                Some("permission-key"),
            ),
            &ExecutionPolicy::read_only(0),
        )
        .await;
    print_trace(&registry);
    eprintln!(
        "OBSERVATION={observation:?} COUNTER={}",
        handle.value("score")
    );
}

async fn lost_response() {
    let tool = CounterTool::with_lost_response_once(10);
    let handle = tool.handle();
    let mut registry = Registry::default();
    registry.register(Box::new(tool)).unwrap();
    let call = request(
        "demo-lost-response",
        "counter_add",
        json!({"key":"score","amount":1}),
        Some("stable-key"),
    );
    let policy = ExecutionPolicy::with_state_changes(0);
    let first = registry.execute(&call, &policy).await;
    let second = registry.execute(&call, &policy).await;
    print_trace(&registry);
    eprintln!(
        "FIRST={first:?} SECOND={second:?} COUNTER={}",
        handle.value("score")
    );
}
