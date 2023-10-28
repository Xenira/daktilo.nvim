use config::Config;
use daktilo_server::client_proto::{daktilo_client::DaktiloClient, ReportCursorMovementRequest};
use nvim_oxi as oxi;
use oxi::{
    api::{self, opts::CreateAutocmdOptsBuilder},
    libuv::AsyncHandle,
    Dictionary, Function,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

mod config;

struct BufInfo {
    col: u64,
    line: u64,
    name: Option<String>,
}

struct MessageEvent {
    err: bool,
    message: String,
}

impl From<anyhow::Error> for MessageEvent {
    fn from(err: anyhow::Error) -> Self {
        Self::new(true, err.to_string())
    }
}

impl From<&str> for MessageEvent {
    fn from(message: &str) -> Self {
        Self::new(false, message.to_string())
    }
}

impl MessageEvent {
    fn new(err: bool, message: String) -> Self {
        Self { err, message }
    }
}

impl From<BufInfo> for ReportCursorMovementRequest {
    fn from(buf_info: BufInfo) -> Self {
        Self {
            column_number: buf_info.col,
            line_number: Some(buf_info.line),
            file_path: buf_info.name,
        }
    }
}

impl BufInfo {
    fn new(name: Option<String>, cursor: (usize, usize)) -> Self {
        Self {
            col: cursor.1 as u64,
            line: cursor.0 as u64 - 1,
            name,
        }
    }
}

fn start(config: Config) -> oxi::Result<()> {
    let (sender, receiver) = unbounded_channel::<BufInfo>();
    let (message_sender, mut message_receiver) = unbounded_channel::<MessageEvent>();

    api::create_autocmd(
        ["CursorMovedI"],
        &CreateAutocmdOptsBuilder::default()
            .callback(move |_| {
                let window = api::get_current_win();
                let cursor = window.get_cursor()?;
                let name = window
                    .get_buf()?
                    .get_name()?
                    .to_str()
                    .map(ToOwned::to_owned);

                let _ = sender.send(BufInfo::new(name, cursor));
                Ok::<bool, oxi::Error>(false)
            })
            .build(),
    )?;

    let handle = AsyncHandle::new(move || {
        let message = message_receiver.blocking_recv().unwrap();
        oxi::schedule(move |_| {
            if message.err {
                api::err_writeln(message.message.as_str());
                return Ok(());
            } else {
                api::out_write(message.message.as_str());
            }
            Ok(())
        });
        Ok::<_, oxi::Error>(())
    })?;

    std::thread::spawn(move || {
        start_grpc_client(config.rpc_port, receiver, message_sender, handle)
    });

    Ok(())
}

#[tokio::main]
async fn start_grpc_client(
    port: u16,
    mut receiver: UnboundedReceiver<BufInfo>,
    message_sender: UnboundedSender<MessageEvent>,
    message_handle: AsyncHandle,
) {
    let addr = format!("http://[::1]:{}", port);
    let mut client = match DaktiloClient::connect(addr.clone()).await {
        Ok(client) => {
            message_sender.send("Connected to server".into()).unwrap();
            client
        }
        Err(e) => {
            message_sender
                .send(anyhow::anyhow!("Failed to connect to server {}: {}", addr, e).into())
                .unwrap();
            message_handle.send().unwrap();
            return;
        }
    };

    message_sender.send("Client connected".into()).unwrap();
    message_handle.send().unwrap();

    while let Some(buf_info) = receiver.recv().await {
        let request: ReportCursorMovementRequest = buf_info.into();
        let _ = client.report_cursor_movement(request).await.unwrap();
    }

    message_sender.send("Client disconnected".into()).unwrap();
    message_handle.send().unwrap();
}

#[oxi::module]
fn daktilo_nvim() -> oxi::Result<Dictionary> {
    Ok(Dictionary::from_iter(vec![(
        "start",
        Function::from_fn(start),
    )]))
}
