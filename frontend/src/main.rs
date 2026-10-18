use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};
use js_sys::JSON;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    id: String,
    sender: String,
    content: String,
    created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CreateMessageRequest {
    sender: String,
    content: String,
}

fn build_ws_url() -> String {
    let window = web_sys::window().unwrap();
    let location = window.location();
    let protocol = if location.protocol().unwrap().starts_with("https") { "wss" } else { "ws" };
    let host = location.host().unwrap_or_else(|_| "localhost:3000".to_string());
    format!("{}://{}/ws", protocol, host)
}

fn app(cx: Scope) -> Element {
    let messages = use_state(cx, Vec::<Message>::new);
    let sender_ref = use_ref(cx, || None::<WebSocket>);
    let name = use_state(cx, || String::new());
    let text = use_state(cx, || String::new());

    use_effect(cx, (), |_| {
        let ws_url = build_ws_url();
        let ws = WebSocket::new(&ws_url).expect("failed to create websocket");

        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let onmessage_messages = messages.clone();

        let onmessage_cb = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().as_string() {
                if let Ok(msg) = serde_json::from_str::<Message>(&txt) {
                    onmessage_messages.make_mut().push(msg);
                }
            }
        });
        ws.set_onmessage(Some(onmessage_cb.as_ref().unchecked_ref()));
        onmessage_cb.forget();

        let onerror_cb = Closure::<dyn FnMut(ErrorEvent)>::new(move |e: ErrorEvent| {
            web_sys::console::error_1(&e.into());
        });
        ws.set_onerror(Some(onerror_cb.as_ref().unchecked_ref()));
        onerror_cb.forget();

        *sender_ref.write_silent() = Some(ws);

        async move {
            // no cleanup for now
            || {}
        }
    });

    cx.render(rsx!(
        div { class: "container",
            h1 { "noicetom (Dioxus)" }
            div { class: "chat",
                div { class: "messages",
                    messages.get().iter().rev().map(|m| rsx!(
                        div { key: "{m.id}", class: "message",
                            b { "{m.sender}: " }
                            span { "{m.content}" }
                            div { class: "time", "{m.created_at}" }
                        }
                    ))
                }
                form { onsubmit: move |e| {
                    e.prevent_default();
                    let ws_option = sender_ref.read();
                    if let Some(ws) = ws_option.as_ref() {
                        let payload = CreateMessageRequest { sender: name.get().clone(), content: text.get().clone() };
                        if let Ok(s) = serde_json::to_string(&payload) {
                            let _ = ws.send_with_str(&s);
                        }
                    }
                    text.set(String::new());
                },
                    input { r#type: "text", placeholder: "Your name", value: "{name}", oninput: move |e| name.set(e.value.clone()) }
                    input { r#type: "text", placeholder: "Message", value: "{text}", oninput: move |e| text.set(e.value.clone()) }
                    button { r#type: "submit", "Send" }
                }
            }
        }
    ))
}

#[wasm_bindgen(start)]
pub fn start() {
    dioxus_web::launch(app)
}
