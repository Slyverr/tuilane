use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::runtime::Builder;

use crate::AssistantConfig;

const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const CANCELLATION_POLL: Duration = Duration::from_millis(25);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct ChatMessage {
    role: String,
    content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
}

impl ChatMessage {
    pub(crate) fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_owned(),
            content: content.into(),
            tool_calls: Vec::new(),
            tool_name: None,
        }
    }

    pub(crate) fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_owned(),
            content: content.into(),
            tool_calls: Vec::new(),
            tool_name: None,
        }
    }

    pub(crate) fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_owned(),
            content: content.into(),
            tool_calls: Vec::new(),
            tool_name: None,
        }
    }

    pub(crate) fn tool(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_owned(),
            content: content.into(),
            tool_calls: Vec::new(),
            tool_name: Some(name.into()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct ToolCall {
    pub(crate) function: ToolFunction,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct ToolFunction {
    pub(crate) name: String,
    pub(crate) arguments: Value,
}

pub(crate) enum ProviderEvent {
    TextDelta(String),
    Finished(ChatMessage),
    Failed(String),
    Cancelled,
}

pub(crate) struct ChatWorker {
    events_tx: Sender<ProviderEvent>,
    events_rx: Receiver<ProviderEvent>,
    cancellation: Option<Arc<AtomicBool>>,
    thread: Option<JoinHandle<()>>,
}

impl ChatWorker {
    pub(crate) fn new() -> Self {
        let (events_tx, events_rx) = mpsc::channel();
        Self {
            events_tx,
            events_rx,
            cancellation: None,
            thread: None,
        }
    }

    pub(crate) fn start(
        &mut self,
        config: &AssistantConfig,
        messages: Vec<ChatMessage>,
    ) -> Result<(), String> {
        if self.is_active() {
            return Err("an assistant response is already in progress".to_owned());
        }
        self.join_finished_thread();

        let endpoint = config
            .endpoint()
            .join("api/chat")
            .map_err(|error| format!("invalid local assistant endpoint: {error}"))?;
        let model = config.model().to_owned();
        let cancellation = Arc::new(AtomicBool::new(false));
        let thread_cancellation = Arc::clone(&cancellation);
        let events = self.events_tx.clone();
        let thread = thread::Builder::new()
            .name("tuilane-assistant".to_owned())
            .spawn(move || {
                let runtime = Builder::new_current_thread().enable_all().build();
                match runtime {
                    Ok(runtime) => runtime.block_on(stream_chat(
                        endpoint,
                        model,
                        messages,
                        thread_cancellation,
                        events.clone(),
                    )),
                    Err(error) => {
                        let _ = events.send(ProviderEvent::Failed(format!(
                            "could not start the assistant network worker: {error}"
                        )));
                    }
                }
            })
            .map_err(|error| format!("could not start the assistant worker: {error}"))?;

        self.cancellation = Some(cancellation);
        self.thread = Some(thread);
        Ok(())
    }

    pub(crate) fn cancel(&self) {
        if let Some(cancellation) = &self.cancellation {
            cancellation.store(true, Ordering::Release);
        }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.cancellation.is_some()
    }

    pub(crate) fn try_recv(&self) -> Result<ProviderEvent, TryRecvError> {
        self.events_rx.try_recv()
    }

    pub(crate) fn finish(&mut self) {
        self.cancellation = None;
        self.join_finished_thread();
    }

    fn join_finished_thread(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ChatWorker {
    fn drop(&mut self) {
        self.cancel();
        self.join_finished_thread();
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    tools: Vec<Value>,
    stream: bool,
}

#[derive(Default, Deserialize)]
struct StreamMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
}

#[derive(Default, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    message: StreamMessage,
    #[serde(default)]
    done: bool,
    error: Option<String>,
}

struct StreamAccumulator {
    pending: Vec<u8>,
    total_bytes: usize,
    message: ChatMessage,
    done: bool,
}

impl StreamAccumulator {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            total_bytes: 0,
            message: ChatMessage::assistant(String::new()),
            done: false,
        }
    }

    fn push(&mut self, chunk: &[u8], events: &Sender<ProviderEvent>) -> Result<(), String> {
        self.total_bytes = self
            .total_bytes
            .checked_add(chunk.len())
            .filter(|length| *length <= MAX_RESPONSE_BYTES)
            .ok_or_else(|| "local assistant response exceeded the 64 KiB limit".to_owned())?;
        self.pending.extend_from_slice(chunk);

        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.pending.drain(..=end).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.process_line(&line, events)?;
            if self.done {
                break;
            }
        }
        Ok(())
    }

    fn finish(&mut self, events: &Sender<ProviderEvent>) -> Result<ChatMessage, String> {
        if !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            self.process_line(&pending, events)?;
        }
        if !self.done {
            return Err("local assistant ended the response before marking it complete".to_owned());
        }
        Ok(std::mem::replace(
            &mut self.message,
            ChatMessage::assistant(String::new()),
        ))
    }

    fn process_line(&mut self, line: &[u8], events: &Sender<ProviderEvent>) -> Result<(), String> {
        if line.iter().all(u8::is_ascii_whitespace) {
            return Ok(());
        }
        let chunk = serde_json::from_slice::<StreamChunk>(line)
            .map_err(|_| "local assistant returned invalid streaming JSON".to_owned())?;
        if let Some(error) = chunk.error {
            return Err(format!("local assistant error: {error}"));
        }
        if !chunk.message.content.is_empty() {
            if self.message.content.len() + chunk.message.content.len() > MAX_RESPONSE_BYTES {
                return Err("local assistant text exceeded the 64 KiB limit".to_owned());
            }
            self.message.content.push_str(&chunk.message.content);
            events
                .send(ProviderEvent::TextDelta(chunk.message.content))
                .map_err(|_| "assistant view closed while a response was streaming".to_owned())?;
        }
        self.message.tool_calls.extend(chunk.message.tool_calls);
        self.done |= chunk.done;
        Ok(())
    }
}

async fn stream_chat(
    endpoint: Url,
    model: String,
    messages: Vec<ChatMessage>,
    cancellation: Arc<AtomicBool>,
    events: Sender<ProviderEvent>,
) {
    let result = stream_chat_inner(endpoint, &model, &messages, &cancellation, &events).await;
    match result {
        Ok(message) => {
            let _ = events.send(ProviderEvent::Finished(message));
        }
        Err(StreamError::Cancelled) => {
            let _ = events.send(ProviderEvent::Cancelled);
        }
        Err(StreamError::Failed(message)) => {
            let _ = events.send(ProviderEvent::Failed(message));
        }
    }
}

enum StreamError {
    Cancelled,
    Failed(String),
}

async fn stream_chat_inner(
    endpoint: Url,
    model: &str,
    messages: &[ChatMessage],
    cancellation: &AtomicBool,
    events: &Sender<ProviderEvent>,
) -> Result<ChatMessage, StreamError> {
    let client = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|error| {
            StreamError::Failed(format!("could not create local provider client: {error}"))
        })?;
    let request = client.post(endpoint).json(&ChatRequest {
        model,
        messages,
        tools: tool_definitions(),
        stream: true,
    });
    let response = tokio::select! {
        response = request.send() => response.map_err(|error| {
            StreamError::Failed(format!("could not reach the local model provider: {error}"))
        })?,
        () = wait_for_cancellation(cancellation) => return Err(StreamError::Cancelled),
    };
    if !response.status().is_success() {
        return Err(StreamError::Failed(format!(
            "local model provider returned HTTP {}",
            response.status()
        )));
    }

    let mut response = response;
    let mut accumulator = StreamAccumulator::new();
    loop {
        let chunk = tokio::select! {
            chunk = response.chunk() => chunk.map_err(|error| {
                StreamError::Failed(format!("could not read local assistant response: {error}"))
            })?,
            () = wait_for_cancellation(cancellation) => return Err(StreamError::Cancelled),
        };
        let Some(chunk) = chunk else {
            break;
        };
        accumulator
            .push(&chunk, events)
            .map_err(StreamError::Failed)?;
        if accumulator.done {
            break;
        }
    }

    accumulator.finish(events).map_err(StreamError::Failed)
}

async fn wait_for_cancellation(cancellation: &AtomicBool) {
    loop {
        if cancellation.load(Ordering::Acquire) {
            return;
        }
        tokio::time::sleep(CANCELLATION_POLL).await;
    }
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "list_open_tasks",
                "description": "List open tasks from the local tuilane workspace.",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "create_task",
                "description": "Propose creating one task. The user must approve it.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Task title" }
                    },
                    "required": ["title"],
                    "additionalProperties": false
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "complete_task",
                "description": "Propose completing one open task. The user must approve it.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "task_id": { "type": "string", "description": "Task UUID" }
                    },
                    "required": ["task_id"],
                    "additionalProperties": false
                }
            }
        }),
    ]
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::mpsc,
        time::Instant,
    };

    use super::*;
    use crate::AssistantConfig;

    #[test]
    fn streaming_accumulator_combines_text_and_tool_calls_across_lines() {
        let (sender, _receiver) = mpsc::channel();
        let mut accumulator = StreamAccumulator::new();
        accumulator
            .push(
                br#"{"message":{"content":"Hello "},"done":false}
{"message":{"content":"there","tool_calls":[{"function":{"name":"list_open_tasks","arguments":{}}}]},"done":true}
"#,
                &sender,
            )
            .unwrap();

        let response = accumulator.finish(&sender).unwrap();
        assert_eq!(response.content, "Hello there");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].function.name, "list_open_tasks");
    }

    #[test]
    fn streaming_accumulator_rejects_incomplete_responses() {
        let (sender, _receiver) = mpsc::channel();
        let mut accumulator = StreamAccumulator::new();
        accumulator
            .push(
                br#"{"message":{"content":"partial"},"done":false}"#,
                &sender,
            )
            .unwrap();

        assert!(accumulator.finish(&sender).is_err());
    }

    #[test]
    fn advertised_tools_exclude_destructive_operations() {
        let tools = tool_definitions();
        let names = tools
            .iter()
            .filter_map(|tool| tool["function"]["name"].as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, ["list_open_tasks", "create_task", "complete_task"]);
    }

    #[test]
    fn local_provider_streams_text_and_tool_calls() {
        let body = br#"{"message":{"content":"Hello "},"done":false}
{"message":{"content":"there","tool_calls":[{"function":{"name":"list_open_tasks","arguments":{}}}]},"done":true}
"#;
        let (endpoint, received_request, server) = mock_server(body, Duration::ZERO);
        let config = AssistantConfig::new("test-model", endpoint).unwrap();
        let mut worker = ChatWorker::new();
        worker
            .start(&config, vec![ChatMessage::user("Summarize my tasks")])
            .unwrap();
        let request = received_request
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        let response = collect_until_finished(&worker);
        worker.finish();
        server.join().unwrap();

        assert!(request.contains("\"stream\":true"));
        assert!(request.contains("list_open_tasks"));
        assert!(request.contains("create_task"));
        assert!(!request.contains("delete_task"));
        assert_eq!(response.content, "Hello there");
        assert_eq!(response.tool_calls.len(), 1);
    }

    #[test]
    fn local_provider_cancels_while_waiting_for_response_headers() {
        let (endpoint, received_request, server) = mock_server(b"", Duration::from_millis(500));
        let config = AssistantConfig::new("test-model", endpoint).unwrap();
        let mut worker = ChatWorker::new();
        worker
            .start(&config, vec![ChatMessage::user("Cancel this request")])
            .unwrap();
        let _request = received_request
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        let cancelled_at = Instant::now();
        worker.cancel();

        let event = receive_terminal_event(&worker);
        let cancellation_latency = cancelled_at.elapsed();
        worker.finish();
        let _ = server.join();

        assert!(matches!(event, ProviderEvent::Cancelled));
        assert!(cancellation_latency < Duration::from_millis(250));
    }

    #[test]
    fn local_provider_cancels_after_streaming_has_started() {
        let prefix = b"{\"message\":{\"content\":\"partial\"},\"done\":false}\n";
        let suffix = b"{\"message\":{\"content\":\" later\"},\"done\":true}\n";
        let (endpoint, received_request, first_chunk, server) =
            mock_streaming_server(prefix, suffix, Duration::from_millis(500));
        let config = AssistantConfig::new("test-model", endpoint).unwrap();
        let mut worker = ChatWorker::new();
        worker
            .start(&config, vec![ChatMessage::user("Stream then cancel")])
            .unwrap();
        let _request = received_request
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        first_chunk.recv_timeout(Duration::from_secs(2)).unwrap();
        let streamed_text = receive_text_delta(&worker);
        let cancelled_at = Instant::now();
        worker.cancel();

        let event = receive_terminal_event(&worker);
        let cancellation_latency = cancelled_at.elapsed();
        worker.finish();
        let _ = server.join();

        assert_eq!(streamed_text, "partial");
        assert!(matches!(event, ProviderEvent::Cancelled));
        assert!(cancellation_latency < Duration::from_millis(250));
    }

    fn collect_until_finished(worker: &ChatWorker) -> ChatMessage {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut response = None;
        let mut streamed_text = String::new();
        while Instant::now() < deadline {
            match worker.try_recv() {
                Ok(ProviderEvent::TextDelta(text)) => streamed_text.push_str(&text),
                Ok(ProviderEvent::Finished(message)) => {
                    response = Some(message);
                    break;
                }
                Ok(ProviderEvent::Failed(error)) => panic!("provider failed: {error}"),
                Ok(ProviderEvent::Cancelled) => panic!("provider was unexpectedly cancelled"),
                Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(5)),
                Err(TryRecvError::Disconnected) => panic!("provider event channel disconnected"),
            }
        }
        let response = response.expect("provider should finish before the test timeout");
        assert_eq!(streamed_text, response.content);
        response
    }

    fn receive_terminal_event(worker: &ChatWorker) -> ProviderEvent {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match worker.try_recv() {
                Ok(event @ ProviderEvent::Finished(_))
                | Ok(event @ ProviderEvent::Failed(_))
                | Ok(event @ ProviderEvent::Cancelled) => return event,
                Ok(ProviderEvent::TextDelta(_)) | Err(TryRecvError::Empty) => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(TryRecvError::Disconnected) => panic!("provider event channel disconnected"),
            }
        }
        panic!("provider did not stop after cancellation");
    }

    fn receive_text_delta(worker: &ChatWorker) -> String {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match worker.try_recv() {
                Ok(ProviderEvent::TextDelta(text)) => return text,
                Ok(ProviderEvent::Failed(error)) => panic!("provider failed: {error}"),
                Ok(ProviderEvent::Finished(_)) => {
                    panic!("provider finished before the first delta")
                }
                Ok(ProviderEvent::Cancelled) => {
                    panic!("provider was cancelled before the first delta")
                }
                Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(5)),
                Err(TryRecvError::Disconnected) => panic!("provider event channel disconnected"),
            }
        }
        panic!("provider did not stream the first text delta");
    }

    fn mock_server(
        body: &'static [u8],
        delay: Duration,
    ) -> (String, Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (request_tx, request_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            let _ = request_tx.send(request);
            if !delay.is_zero() {
                thread::sleep(delay);
            }
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if stream.write_all(header.as_bytes()).is_err() {
                return;
            }
            for chunk in body.chunks(7) {
                if stream.write_all(chunk).is_err() {
                    return;
                }
                let _ = stream.flush();
            }
        });
        (format!("http://{address}"), request_rx, server)
    }

    fn mock_streaming_server(
        prefix: &'static [u8],
        suffix: &'static [u8],
        delay: Duration,
    ) -> (String, Receiver<String>, Receiver<()>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (request_tx, request_rx) = mpsc::channel();
        let (chunk_tx, chunk_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            let _ = request_tx.send(request);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                prefix.len() + suffix.len()
            );
            if stream.write_all(header.as_bytes()).is_err()
                || stream.write_all(prefix).is_err()
                || stream.flush().is_err()
            {
                return;
            }
            let _ = chunk_tx.send(());
            thread::sleep(delay);
            let _ = stream.write_all(suffix);
        });
        (format!("http://{address}"), request_rx, chunk_rx, server)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        let mut expected_size = None;
        loop {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if expected_size.is_none()
                && let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n")
            {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                });
                expected_size = Some(header_end + 4 + content_length.unwrap_or(0));
            }
            if expected_size.is_some_and(|size| request.len() >= size) {
                break;
            }
        }
        String::from_utf8_lossy(&request).into_owned()
    }
}
