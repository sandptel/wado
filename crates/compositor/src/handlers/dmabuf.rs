//! `zwp_linux_dmabuf_v1` — let clients hand the compositor GPU buffers.
//!
//! Without this global `wl_shm` is the only buffer path a client has, so a GPU application
//! renders on the GPU, reads back to CPU memory, writes it into shared memory, and the
//! compositor uploads it to a texture again — two full copies of every window, every frame,
//! on the render tick. At a phone's 1080×2422 that is ~10 MB per surface per frame.
//!
//! The import itself is the renderer's job; this module only routes the client's request to
//! it and answers. **The answer is not optional**: smithay warns "Compositor bug: Server
//! ignored ImportNotifier" if the notifier is dropped, and the client waits forever.

use smithay::backend::allocator::Buffer as _;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::ImportDma;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};

use crate::Wado;

impl DmabufHandler for Wado {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        // No renderer means no session, and the global is only advertised while one is
        // running — but a buffer already in flight when a session stops lands here, and
        // `failed()` is the honest answer for it.
        let Some(renderer) = self.renderer.as_mut() else {
            notifier.failed();
            return;
        };
        let first = !self.dmabuf_logged;
        self.dmabuf_logged = true;
        // Imported eagerly rather than at first use so a modifier the driver refuses is
        // reported as a protocol error now, while the client can still fall back to shm.
        // Deferring it turns the same failure into a blank window at render time.
        match renderer.import_dmabuf(&dmabuf, None) {
            Ok(_) => {
                if first {
                    let f = dmabuf.format();
                    let windows = self.space.elements().count();
                    tracing::info!(
                        format = %f.code,
                        modifier = ?f.modifier,
                        windows,
                        "dmabuf path is live — a client is handing over GPU buffers"
                    );
                }
                let _ = notifier.successful::<Wado>();
            }
            Err(e) => {
                // `failed()` on a `create_immed` is a fatal protocol error, not a fallback to
                // shm — the client dies. So this line means "the app you launched just
                // disappeared", which is worth saying every time rather than once.
                tracing::warn!("dmabuf import rejected, the client will be killed: {e}");
                notifier.failed();
            }
        }
    }
}
