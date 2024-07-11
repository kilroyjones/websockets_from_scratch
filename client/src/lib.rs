use wasm_bindgen::prelude::*;
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[wasm_bindgen(start)]
fn start_websocket() -> Result<(), JsValue> {
    // Connect to an echo server
    let ws = WebSocket::new("ws://127.0.0.1:8080")?;
    // For small binary messages, like CBOR, Arraybuffer is more efficient than Blob handling
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
    // create callback
    let cloned_ws = ws.clone();
    let onmessage_callback = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        // Handle difference Text/Binary,...
        if let Ok(abuf) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
            console_log!("message event, received arraybuffer: {:?}", abuf);
            let array = js_sys::Uint8Array::new(&abuf);
            let len = array.byte_length() as usize;
            console_log!("Arraybuffer received {}bytes: {:?}", len, array.to_vec());
            // here you can for example use Serde Deserialize decode the message
            // for demo purposes we switch back to Blob-type and send off another binary message
            cloned_ws.set_binary_type(web_sys::BinaryType::Blob);
            match cloned_ws.send_with_u8_array(&[5, 6, 7, 8]) {
                Ok(_) => console_log!("binary message successfully sent"),
                Err(err) => console_log!("error sending message: {:?}", err),
            }
        } else if let Ok(blob) = e.data().dyn_into::<web_sys::Blob>() {
            console_log!("message event, received blob: {:?}", blob);
            // better alternative to juggling with FileReader is to use https://crates.io/crates/gloo-file
            let fr = web_sys::FileReader::new().unwrap();
            let fr_c = fr.clone();
            // create onLoadEnd callback
            let onloadend_cb = Closure::<dyn FnMut(_)>::new(move |_e: web_sys::ProgressEvent| {
                let array = js_sys::Uint8Array::new(&fr_c.result().unwrap());
                let len = array.byte_length() as usize;
                console_log!("Blob received {}bytes: {:?}", len, array.to_vec());
                // here you can for example use the received image/png data
            });
            fr.set_onloadend(Some(onloadend_cb.as_ref().unchecked_ref()));
            fr.read_as_array_buffer(&blob).expect("blob not readable");
            onloadend_cb.forget();
        } else if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
            console_log!("message event, received Text: {:?}", txt);
        } else {
            console_log!("message event, received Unknown: {:?}", e.data());
        }
    });
    // set message event handler on WebSocket
    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    // forget the callback to keep it alive
    onmessage_callback.forget();

    let onerror_callback = Closure::<dyn FnMut(_)>::new(move |e: ErrorEvent| {
        console_log!("error event: {:?}", e);
    });
    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
    onerror_callback.forget();

    let cloned_ws = ws.clone();
    let onopen_callback = Closure::<dyn FnMut()>::new(move || {
        console_log!("socket opened");
        match cloned_ws.send_with_str("ping") {
            Ok(_) => console_log!("message successfully sent"),
            Err(err) => console_log!("error sending message: {:?}", err),
        }
        // send off binary message
        match cloned_ws.send_with_u8_array(&[0, 1, 2, 3]) {
            Ok(_) => console_log!("binary message successfully sent"),
            Err(err) => console_log!("error sending message: {:?}", err),
        }
    });
    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();

    Ok(())
}

// use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
// use futures_util::{SinkExt, StreamExt};
// use wasm_bindgen::prelude::*;
// use wasm_bindgen::JsCast;
// use web_sys::{ErrorEvent, MessageEvent, WebSocket};

// #[wasm_bindgen]
// extern "C" {
//     #[wasm_bindgen(js_namespace = console)]
//     fn log(s: &str);
// }

// #[wasm_bindgen]
// #[derive(Clone)]
// pub struct WsClient {
//     ws: WebSocket,
//     sender: UnboundedSender<String>,
//     is_connected: bool,
// }

// #[wasm_bindgen]
// impl WsClient {
//     #[wasm_bindgen(constructor)]
//     pub fn new(url: &str) -> Result<WsClient, JsValue> {
//         log("Initializing WebSocket connection...");
//         let ws = WebSocket::new(url)?;

//         let (tx, mut rx): (UnboundedSender<String>, UnboundedReceiver<String>) = unbounded();

//         let tx_clone = tx.clone();

//         let onopen_callback = Closure::wrap(Box::new(move || {
//             log("WebSocket connection established");
//         }) as Box<dyn FnMut()>);
//         ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
//         onopen_callback.forget();

//         let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
//             if let Some(text) = e.data().as_string() {
//                 log(&format!("Received message: {}", text));
//                 tx_clone.unbounded_send(text).unwrap();
//             }
//         }) as Box<dyn FnMut(MessageEvent)>);
//         ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
//         onmessage_callback.forget();

//         let onerror_callback = Closure::wrap(Box::new(move |e: ErrorEvent| {
//             log(&format!("WebSocket error: {}", e.message()));
//         }) as Box<dyn FnMut(ErrorEvent)>);
//         ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
//         onerror_callback.forget();

//         let onclose_callback = Closure::wrap(Box::new(move || {
//             log("WebSocket connection closed");
//         }) as Box<dyn FnMut()>);
//         ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
//         onclose_callback.forget();

//         let ws_clone = ws.clone();

//         let client = WsClient {
//             ws,
//             sender: tx,
//             is_connected: false,
//         };

//         let client_clone = client.clone();

//         wasm_bindgen_futures::spawn_local(async move {
//             while let Some(msg) = rx.next().await {
//                 if client_clone.is_connected {
//                     log(&format!("Sending message: {}", msg));
//                     match ws_clone.send_with_str(&msg) {
//                         Ok(_) => log("Message sent successfully"),
//                         Err(err) => log(&format!("Failed to send message: {:?}", err)),
//                     }
//                 }
//             }
//         });

//         Ok(client)
//     }

//     #[wasm_bindgen]
//     pub fn send_message(&mut self, msg: String) {
//         if self.is_connected {
//             log(&format!("Queuing message: {}", msg));
//             self.sender.unbounded_send(msg).unwrap();
//         } else {
//             log("WebSocket is not connected yet");
//         }
//     }
// }
