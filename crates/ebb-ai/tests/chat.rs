use ebb_ai::*;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;

/// Sahte OpenAI uyumlu sunucu: sırayla verilen yanıtları döner, istekleri kaydeder.
fn fake(responses: Vec<Value>) -> (String, std::thread::JoinHandle<Vec<Value>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let h = std::thread::spawn(move || {
        let mut seen = vec![];
        for r in responses {
            let (mut s, _) = l.accept().unwrap();
            let mut buf = vec![];
            let mut chunk = [0u8; 4096];
            let body = loop {
                let n = s.read(&mut chunk).unwrap();
                buf.extend_from_slice(&chunk[..n]);
                let t = String::from_utf8_lossy(&buf).to_string();
                if let Some(p) = t.find("\r\n\r\n") {
                    let len = t[..p]
                        .to_lowercase()
                        .split("content-length:")
                        .nth(1)
                        .and_then(|x| x.lines().next()?.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if buf.len() >= p + 4 + len {
                        break buf[p + 4..p + 4 + len].to_vec();
                    }
                }
            };
            seen.push(serde_json::from_slice(&body).unwrap());
            let out = r.to_string();
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len()).unwrap();
        }
        seen
    });
    (url, h)
}

fn provider(url: String) -> Provider {
    Provider {
        id: "lmstudio".into(),
        name: "fake".into(),
        base_url: url,
        models: vec![],
        online: true,
    }
}

#[test]
fn tool_call_then_text() {
    let call = json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[
        {"id":"c1","type":"function","function":{"name":"findings","arguments":"{}"}}]}}]});
    let fin =
        json!({"choices":[{"message":{"role":"assistant","content":"You can free some caches."}}]});
    let (url, h) = fake(vec![call, fin]);
    let r = chat(&provider(url), "m", &[], "what can I clean?").unwrap();
    assert_eq!(r.text, "You can free some caches.");
    assert_eq!(r.tool_calls.len(), 1);
    assert_eq!(r.tool_calls[0].name, "findings");
    assert!(r.plan_id.is_none());
    let seen = h.join().unwrap();
    assert!(seen[0]["tools"].is_array());
    // Ikinci istekte araç sonucu "tool" rolüyle gitti.
    assert_eq!(
        seen[1]["messages"].as_array().unwrap().last().unwrap()["role"],
        "tool"
    );
}

#[test]
fn parsers() {
    let o = json!({"models":[{"name":"llama3:8b"},{"name":"qwen"}]});
    assert_eq!(parse_ollama_tags(&o), ["llama3:8b", "qwen"]);
    let l = json!({"data":[{"id":"a"}]});
    assert_eq!(parse_openai_models(&l), ["a"]);
    assert!(parse_ollama_tags(&json!({})).is_empty());
}

#[test]
fn no_destructive_tools() {
    let s = tools_schema().to_string().to_lowercase();
    for bad in ["apply", "delete", "undo", "purge", "remove", "trash"] {
        assert!(!s.contains(&format!("\"name\":\"{bad}")), "{bad}");
    }
    let names: Vec<String> = tools_schema()
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap().to_string())
        .collect();
    for n in &names {
        for bad in ["apply", "delete", "undo", "purge", "remove", "trash"] {
            assert!(!n.contains(bad), "{n}");
        }
    }
    assert!(names.contains(&"plan_cleanup".to_string()));
}

#[test]
fn unknown_tool_errors() {
    assert!(run_tool("unknown", &json!({})).is_err());
    assert!(run_tool("apply_plan", &json!({})).is_err());
}
