//! Accepts files dropped onto the window. Neither winit nor `iced_layershell` takes drops on
//! Wayland, so a data device on the window's own connection does: only that connection's surfaces
//! receive the drag.

mod uri_list;

use std::hash::{Hash, Hasher};
use std::io::Read;
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::sync::Mutex;

use iced::futures::Stream;
use iced::futures::channel::mpsc::{UnboundedSender, unbounded};
use iced::window::raw_window_handle::RawDisplayHandle;
use wayland_client::globals::{BindError, GlobalError, GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{
    Connection, Dispatch, DispatchError, Proxy, QueueHandle, delegate_noop, event_created_child,
};

const URI_LIST: &str = "text/uri-list";
/// The first version with drag actions; compositors cancel drops that never pick one.
const ACTIONS: u32 = 3;

#[derive(Debug, thiserror::Error)]
enum ListenError {
    #[error("cannot list the compositor's globals: {0}")]
    Globals(#[from] GlobalError),
    #[error("no data device manager: {0}")]
    Bind(#[from] BindError),
    #[error("the Wayland connection failed: {0}")]
    Dispatch(#[from] DispatchError),
}

/// The Wayland connection the window lives on.
#[derive(Debug, Clone)]
pub struct DropTarget(Connection);

/// One app has one target; its subscription never needs to restart.
impl Hash for DropTarget {
    fn hash<H: Hasher>(&self, state: &mut H) {
        "dropshade-drop-target".hash(state);
    }
}

impl DropTarget {
    pub fn new(connection: Connection) -> Self {
        Self(connection)
    }

    /// The connection of a window winit opened, if it is a Wayland one.
    #[expect(
        unsafe_code,
        reason = "winit keeps its Wayland connection private; only the raw display is shared"
    )]
    pub fn of_window(window: &dyn iced::window::Window) -> Option<Self> {
        let RawDisplayHandle::Wayland(handle) = window.display_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: the display belongs to winit's event loop, which outlives every window and runs
        // until the app exits; a guest backend never closes it.
        let backend = unsafe {
            wayland_backend::sys::client::Backend::from_foreign_display(
                handle.display.as_ptr().cast(),
            )
        };
        Some(Self(Connection::from_backend(backend)))
    }
}

/// The files dropped onto the window, one list per drop.
pub fn drops(target: &DropTarget) -> impl Stream<Item = Vec<PathBuf>> + use<> {
    let (sender, receiver) = unbounded();
    let connection = target.0.clone();
    let spawned = std::thread::Builder::new()
        .name("drag-and-drop".to_owned())
        .spawn(move || {
            if let Err(error) = listen(&connection, sender) {
                tracing::warn!(%error, "dropped files are ignored");
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "cannot listen for dropped files");
    }
    receiver
}

fn listen(
    connection: &Connection,
    sender: UnboundedSender<Vec<PathBuf>>,
) -> Result<(), ListenError> {
    let (globals, mut queue) = registry_queue_init::<Receiver>(connection)?;
    let handle = queue.handle();
    let manager: WlDataDeviceManager = globals.bind(&handle, 1..=ACTIONS, ())?;
    let seats: Vec<WlSeat> = globals.contents().with_list(|list| {
        list.iter()
            .filter(|global| global.interface == WlSeat::interface().name)
            .map(|global| globals.registry().bind(global.name, 1, &handle, ()))
            .collect()
    });
    let _devices: Vec<WlDataDevice> = seats
        .iter()
        .map(|seat| manager.get_data_device(seat, &handle, ()))
        .collect();
    let mut receiver = Receiver {
        connection: connection.clone(),
        sender,
        offer: None,
    };
    while !receiver.sender.is_closed() {
        queue.blocking_dispatch(&mut receiver)?;
    }
    Ok(())
}

/// The MIME types a data offer announced.
type Mimes = Mutex<Vec<String>>;

#[derive(Debug)]
struct Receiver {
    connection: Connection,
    sender: UnboundedSender<Vec<PathBuf>>,
    /// The offer of the drag over the window.
    offer: Option<WlDataOffer>,
}

impl Receiver {
    fn enter(&mut self, serial: u32, offer: Option<WlDataOffer>) {
        self.leave();
        let Some(offer) = offer else { return };
        let files = offers_files(&offer);
        offer.accept(serial, files.then(|| URI_LIST.to_owned()));
        if files && offer.version() >= ACTIONS {
            offer.set_actions(DndAction::Copy, DndAction::Copy);
        }
        self.offer = Some(offer);
    }

    fn leave(&mut self) {
        if let Some(offer) = self.offer.take() {
            offer.destroy();
        }
    }

    fn drop_files(&mut self) {
        let Some(offer) = self.offer.take() else {
            return;
        };
        if !offers_files(&offer) {
            offer.destroy();
            return;
        }
        let (reader, writer) = match std::io::pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                tracing::warn!(%error, "cannot receive the dropped files");
                offer.destroy();
                return;
            }
        };
        offer.receive(URI_LIST.to_owned(), writer.as_fd());
        drop(writer);
        flush(&self.connection);
        // The source writes at its own pace; reading here would stall the other drag events.
        let sender = self.sender.clone();
        let connection = self.connection.clone();
        let spawned = std::thread::Builder::new()
            .name("drop-read".to_owned())
            .spawn(move || read_drop(reader, &offer, &sender, &connection));
        if let Err(error) = spawned {
            tracing::warn!(%error, "cannot read the dropped files");
        }
    }
}

fn read_drop(
    mut reader: std::io::PipeReader,
    offer: &WlDataOffer,
    sender: &UnboundedSender<Vec<PathBuf>>,
    connection: &Connection,
) {
    let mut bytes = Vec::new();
    match reader.read_to_end(&mut bytes) {
        Ok(_) => {
            let paths = uri_list::paths(&String::from_utf8_lossy(&bytes));
            if !paths.is_empty() && sender.unbounded_send(paths).is_err() {
                tracing::debug!("app already shutting down");
            }
        }
        Err(error) => tracing::warn!(%error, "cannot read the dropped files"),
    }
    if offer.version() >= ACTIONS {
        offer.finish();
    }
    offer.destroy();
    flush(connection);
}

fn flush(connection: &Connection) {
    if let Err(error) = connection.flush() {
        tracing::warn!(%error, "cannot answer the drag");
    }
}

fn offers_files(offer: &WlDataOffer) -> bool {
    offer.data::<Mimes>().is_some_and(|mimes| {
        mimes
            .lock()
            .is_ok_and(|mimes| mimes.iter().any(|mime| mime == URI_LIST))
    })
}

impl Dispatch<WlDataDevice, ()> for Receiver {
    fn event(
        state: &mut Self,
        _device: &WlDataDevice,
        event: wl_data_device::Event,
        (): &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::Enter { serial, id, .. } => state.enter(serial, id),
            wl_data_device::Event::Leave => state.leave(),
            wl_data_device::Event::Drop => state.drop_files(),
            // The clipboard has its own connection; these offers are never read here.
            wl_data_device::Event::Selection { id: Some(offer) } => offer.destroy(),
            _ => {}
        }
    }

    event_created_child!(Receiver, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, Mimes::default()),
    ]);
}

impl Dispatch<WlDataOffer, Mimes> for Receiver {
    fn event(
        _state: &mut Self,
        _offer: &WlDataOffer,
        event: wl_data_offer::Event,
        mimes: &Mimes,
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event
            && let Ok(mut mimes) = mimes.lock()
        {
            mimes.push(mime_type);
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Receiver {
    fn event(
        _state: &mut Self,
        _registry: &WlRegistry,
        _event: wl_registry::Event,
        _contents: &GlobalListContents,
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Receiver: WlDataDeviceManager);
delegate_noop!(Receiver: ignore WlSeat);
