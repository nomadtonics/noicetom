use dioxus::prelude::*;
use noicetom::{Message, Storage};

fn app(cx: Scope) -> Element {
    let storage = use_ref(cx, || Storage::new().expect("failed to init storage"));
    let messages = use_state(cx, || storage.read().load_messages().unwrap_or_default());
    let name = use_state(cx, || String::new());
    let text = use_state(cx, || String::new());

    let send_message = {
        let storage = storage.clone();
        let messages = messages.clone();
        let name = name.clone();
        let text = text.clone();
        move |_| {
            let sender = name.get().trim().to_string();
            let content = text.get().trim().to_string();
            if sender.is_empty() || content.is_empty() {
                return;
            }

            let message = Message {
                id: uuid::Uuid::new_v4().to_string(),
                sender: sender.clone(),
                content: content.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            let mut next = messages.get().clone();
            next.push(message.clone());
            messages.set(next);

            if let Err(err) = storage.read().append_message(&message) {
                eprintln!("storage error: {}", err);
            }
            text.set(String::new());
        }
    };

    cx.render(rsx!(
        div { style: "font-family: Arial, sans-serif; padding: 24px; max-width: 780px; margin: 0 auto;",
            h1 { "noicetom" }
            p { "Desktop Rust chat app with encrypted local storage." }
            div { style: "display:flex; gap:12px; margin-bottom: 12px; flex-wrap: wrap;",
                input {
                    placeholder: "Your name",
                    value: "{name}",
                    oninput: move |evt| name.set(evt.value.clone()),
                    style: "padding:8px; min-width: 160px;"
                }
                input {
                    placeholder: "Message",
                    value: "{text}",
                    oninput: move |evt| text.set(evt.value.clone()),
                    style: "padding:8px; flex: 1; min-width: 200px;"
                }
                button {
                    onclick: send_message,
                    style: "padding: 8px 16px;",
                    "Send"
                }
            }
            div {
                style: "border: 1px solid #ddd; border-radius: 8px; padding: 12px; background: #fafafa;",
                messages.get().iter().rev().map(|m| rsx!(
                    div { key: "{m.id}", style: "padding: 8px 0; border-bottom: 1px solid #eee;",
                        strong { "{m.sender}: " }
                        span { "{m.content}" }
                        div { style: "font-size: 0.75rem; color: #666; margin-top: 4px;", "{m.created_at}" }
                    }
                ))
            }
        }
    ))
}

fn main() {
    dioxus_desktop::launch(app);
}
