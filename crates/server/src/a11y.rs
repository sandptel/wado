//! Tap targets from an application's accessibility tree (M-P S5).
//!
//! One job: given the window under a finger (from the compositor, `wado_compositor::hit`),
//! list the actionable elements — buttons, toggles, menu items, links, fields — whose extents
//! lie within a fingertip of the point. The client uses the answer to snap a tap onto the one
//! target it obviously meant, or to open the lens when several compete.
//!
//! Everything here was learned by measuring a live GTK4 app in the S5 spike
//! (`examples/a11y_spike.rs`, 2026-09-29), and each rule is load-bearing:
//!
//! - **Hit-test by extents, ourselves.** GTK4's `Component.GetAccessibleAtPoint` returns null
//!   below the window's first container, so the tree is walked and each child's extents
//!   (window coordinates) compared with the finger.
//! - **`ChildCount` + `GetChildAtIndex`, never `GetChildren`.** GTK4 creates child accessibles
//!   lazily; `GetChildren` lists only those already created, which is usually almost none.
//! - **Distrust extents.** Some containers report nonsense (a "tab panel" 1,545,179,744 px
//!   wide). Such a node is descended through — its children are often fine — but never
//!   offered as a target.
//! - **Find the app by process id.** Each app is its own connection on the bus;
//!   `GetConnectionUnixProcessID` maps the window's client pid to it. No registry query needed.
//!
//! Bounded by a call budget and a deadline: a tap must never wait on a pathological tree.

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::debug;
use wado_compositor::hit::HitWindow;
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

/// Roles a finger means to hit. Anything else is a container to look inside.
const ACTIONABLE: &[&str] = &[
    "push button",
    "button",
    "toggle button",
    "check box",
    "radio button",
    "menu item",
    "check menu item",
    "radio menu item",
    "link",
    "combo box",
    "entry",
    "text",
    "password text",
    "spin button",
    "slider",
    "page tab",
    "list item",
    // GTK4's GridView/ColumnView items (file icons, table rows). Role name measured on a live
    // nautilus, not taken from the spec — "table cell" is what an older toolkit says.
    "grid cell",
    "table cell",
    "tree item",
    "switch",
    "icon",
];
/// Most D-Bus calls one query may make; a normal header bar takes ~25.
const CALL_BUDGET: usize = 600;
/// A query that is not back by now is late for the tap it serves.
const DEADLINE: Duration = Duration::from_millis(250);
/// Children examined per node; a list with thousands of rows is not searched row by row.
const MAX_CHILDREN: i32 = 200;

/// An actionable element, in the window's own logical pixels.
#[derive(Debug, Clone)]
pub struct LocalTarget {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub role: String,
    pub name: String,
}

/// Connections and lookups kept between taps: the bus, and which connection each pid is.
#[derive(Default)]
pub struct A11y {
    conn: Mutex<Option<(String, Connection)>>,
    names: Mutex<HashMap<i32, String>>,
}

impl A11y {
    /// Actionable elements within `r` window-logical pixels of the hit point. `None`: the app
    /// has no tree on the bus (or it could not be read in time) — the caller falls back.
    pub async fn targets(&self, hit: &HitWindow, r: f64) -> Option<Vec<LocalTarget>> {
        match tokio::time::timeout(DEADLINE, self.query(hit, r)).await {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                debug!("a11y query failed: {e}");
                None
            }
            Err(_) => {
                debug!("a11y query timed out");
                None
            }
        }
    }

    /// The bus connection and the app's name on it, for the window in `hit`. `None`: the app
    /// is not on the bus. Shared with the menu sheet (`crate::menu_sheet`).
    pub(crate) async fn app(&self, hit: &HitWindow) -> zbus::Result<Option<(Connection, String)>> {
        let conn = self.connection(&hit.a11y).await?;
        Ok(self.app_name(&conn, hit.pid).await?.map(|d| (conn, d)))
    }

    async fn query(&self, hit: &HitWindow, r: f64) -> zbus::Result<Option<Vec<LocalTarget>>> {
        let Some((conn, dest)) = self.app(hit).await? else {
            return Ok(None);
        };
        let mut q = Query {
            conn: &conn,
            dest: &dest,
            calls: 0,
        };
        let Some(frame) = q.frame(&hit.title).await? else {
            return Ok(None);
        };
        let (fx, fy, fw, fh) = q.extents(&frame).await?;
        let sane = |e: (i32, i32, i32, i32)| {
            e.2 > 0
                && e.3 > 0
                && e.2 <= fw.max(1) * 2
                && e.3 <= fh.max(1) * 2
                && e.0 > -fw
                && e.1 > -fh
        };
        let _ = (fx, fy);
        let (px, py) = hit.local;
        let finger = (px - r, py - r, px + r, py + r);
        let touches = |e: (i32, i32, i32, i32)| {
            let (x0, y0, x1, y1) = (
                f64::from(e.0),
                f64::from(e.1),
                f64::from(e.0 + e.2),
                f64::from(e.1 + e.3),
            );
            x0 < finger.2 && x1 > finger.0 && y0 < finger.3 && y1 > finger.1
        };

        let mut out = Vec::new();
        let mut stack = vec![frame];
        while let Some(node) = stack.pop() {
            let n = q.child_count(&node).await?.min(MAX_CHILDREN);
            for i in 0..n {
                if q.calls > CALL_BUDGET {
                    debug!(found = out.len(), "a11y query hit its call budget");
                    return Ok(Some(out));
                }
                let child = q.child_at(&node, i).await?;
                let e = q.extents(&child).await?;
                if sane(e) {
                    if !touches(e) {
                        continue;
                    }
                    let role = q.role(&child).await?;
                    if ACTIONABLE.contains(&role.as_str()) {
                        out.push(LocalTarget {
                            x: f64::from(e.0),
                            y: f64::from(e.1),
                            w: f64::from(e.2),
                            h: f64::from(e.3),
                            name: q.name(&child).await.unwrap_or_default(),
                            role,
                        });
                        continue; // a button's insides are not separate targets
                    }
                }
                // A container that holds the finger — or one whose extents cannot be trusted,
                // whose children often can — is looked inside.
                stack.push(child);
            }
        }
        debug!(found = out.len(), calls = q.calls, "a11y query");
        Ok(Some(out))
    }

    async fn connection(&self, address: &str) -> zbus::Result<Connection> {
        let mut slot = self.conn.lock().await;
        if let Some((a, c)) = slot.as_ref()
            && a == address
        {
            return Ok(c.clone());
        }
        let c = zbus::connection::Builder::address(address)?.build().await?;
        *slot = Some((address.to_string(), c.clone()));
        // A new bus is a new session: every cached pid → name is stale.
        self.names.lock().await.clear();
        Ok(c)
    }

    /// The bus connection name of process `pid`, scanning the bus on a cache miss.
    async fn app_name(&self, conn: &Connection, pid: i32) -> zbus::Result<Option<String>> {
        if let Some(n) = self.names.lock().await.get(&pid) {
            return Ok(Some(n.clone()));
        }
        let names: Vec<String> = call(
            conn,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ListNames",
            &(),
        )
        .await?;
        let mut found = None;
        for n in names.into_iter().filter(|n| n.starts_with(':')) {
            let p: zbus::Result<u32> = call(
                conn,
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "GetConnectionUnixProcessID",
                &(n.as_str(),),
            )
            .await;
            if let Ok(p) = p
                && p as i32 == pid
            {
                found = Some(n);
                break;
            }
        }
        if let Some(n) = &found {
            self.names.lock().await.insert(pid, n.clone());
        }
        Ok(found)
    }
}

/// How long the compositor may take to say which window is under the point.
const HIT_TIMEOUT: Duration = Duration::from_millis(100);

/// The whole question, as the client asks it: targets within `r` (a fraction of the output's
/// width) of normalized `(x, y)`, answered in normalized output coordinates. `None` means "no
/// tree here" — no window, no accessibility bus, or an app that exposes none.
pub async fn tap_targets(
    cmd_tx: &wado_compositor::CommandSender,
    a11y: &A11y,
    x: f64,
    y: f64,
    r: f64,
) -> Option<Vec<wado_protocol::Target>> {
    let (reply, rx) = tokio::sync::oneshot::channel();
    cmd_tx
        .send(wado_compositor::CompositorCommand::HitWindow { x, y, reply })
        .ok()?;
    let hit = tokio::time::timeout(HIT_TIMEOUT, rx).await.ok()?.ok()??;
    // The radius arrives in output terms; the tree speaks the window's own, which differ by the
    // window's shrink factor (M-P S3).
    let r_local = r * hit.output.0 / hit.factor;
    let local = a11y.targets(&hit, r_local).await?;
    Some(local.into_iter().map(|t| to_output(&hit, t)).collect())
}

/// A window-local rectangle, as normalized output coordinates.
fn to_output(hit: &HitWindow, t: LocalTarget) -> wado_protocol::Target {
    let (ox, oy) = hit.origin;
    let (ow, oh) = hit.output;
    let f = hit.factor;
    wado_protocol::Target {
        x: (ox + t.x * f) / ow,
        y: (oy + t.y * f) / oh,
        w: t.w * f / ow,
        h: t.h * f / oh,
        role: t.role,
        name: t.name,
    }
}

/// One query's calls against one app, counted against the budget.
pub(crate) struct Query<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) dest: &'a str,
    pub(crate) calls: usize,
}

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";

/// AT-SPI state bits (`GetState` returns them as two u32 words).
pub(crate) mod state {
    pub const CHECKED: u32 = 4;
    pub const ENABLED: u32 = 8;
    pub const SENSITIVE: u32 = 24;
    pub const SHOWING: u32 = 25;
    pub const HAS_POPUP: u32 = 42;
    /// Whether `bit` is set in a `GetState` answer.
    pub fn has(words: &[u32], bit: u32) -> bool {
        words
            .get((bit / 32) as usize)
            .is_some_and(|w| w & (1 << (bit % 32)) != 0)
    }
}

impl Query<'_> {
    /// The node's state bits — see [`state`].
    pub(crate) async fn states(&mut self, node: &OwnedObjectPath) -> zbus::Result<Vec<u32>> {
        self.call(node.as_str(), ACCESSIBLE, "GetState", &()).await
    }

    /// The node's text through the Text interface — where GTK4 labels keep what they say — or
    /// `None` when it has none (or does not implement Text).
    pub(crate) async fn text(&mut self, node: &OwnedObjectPath) -> Option<String> {
        let t: String = self
            .call(
                node.as_str(),
                "org.a11y.atspi.Text",
                "GetText",
                &(0i32, -1i32),
            )
            .await
            .ok()?;
        let t = t.trim();
        (!t.is_empty()).then(|| t.to_string())
    }

    /// The node's relations: (AT-SPI relation type, targets).
    pub(crate) async fn relations(
        &mut self,
        node: &OwnedObjectPath,
    ) -> zbus::Result<Vec<(u32, Vec<(String, OwnedObjectPath)>)>> {
        self.call(node.as_str(), ACCESSIBLE, "GetRelationSet", &())
            .await
    }

    /// Perform the node's first action (a menu item's "click").
    pub(crate) async fn activate(&mut self, node: &str) -> zbus::Result<bool> {
        self.call(node, "org.a11y.atspi.Action", "DoAction", &(0i32,))
            .await
    }
    async fn call<B, R>(
        &mut self,
        path: &str,
        iface: &str,
        method: &str,
        body: &B,
    ) -> zbus::Result<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        self.calls += 1;
        call(self.conn, self.dest, path, iface, method, body).await
    }

    /// The app's toplevel whose name is the window title; the first one otherwise.
    pub(crate) async fn frame(&mut self, title: &str) -> zbus::Result<Option<OwnedObjectPath>> {
        let frames: Vec<(String, OwnedObjectPath)> = self
            .call(
                "/org/a11y/atspi/accessible/root",
                ACCESSIBLE,
                "GetChildren",
                &(),
            )
            .await?;
        let mut first = None;
        for (_, path) in frames {
            if !title.is_empty() && self.name(&path).await.is_ok_and(|n| n == title) {
                return Ok(Some(path));
            }
            first.get_or_insert(path);
        }
        Ok(first)
    }

    pub(crate) async fn child_count(&mut self, node: &OwnedObjectPath) -> zbus::Result<i32> {
        let v: OwnedValue = self.prop(node, "ChildCount").await?;
        Ok(i32::try_from(v).unwrap_or(0))
    }

    pub(crate) async fn child_at(
        &mut self,
        node: &OwnedObjectPath,
        i: i32,
    ) -> zbus::Result<OwnedObjectPath> {
        let (_, path): (String, OwnedObjectPath) = self
            .call(node.as_str(), ACCESSIBLE, "GetChildAtIndex", &(i,))
            .await?;
        Ok(path)
    }

    /// Extents in window coordinates (AT-SPI coord type 1).
    pub(crate) async fn extents(
        &mut self,
        node: &OwnedObjectPath,
    ) -> zbus::Result<(i32, i32, i32, i32)> {
        self.call(
            node.as_str(),
            "org.a11y.atspi.Component",
            "GetExtents",
            &(1u32,),
        )
        .await
    }

    pub(crate) async fn role(&mut self, node: &OwnedObjectPath) -> zbus::Result<String> {
        self.call(node.as_str(), ACCESSIBLE, "GetRoleName", &())
            .await
    }

    pub(crate) async fn name(&mut self, node: &OwnedObjectPath) -> zbus::Result<String> {
        let v: OwnedValue = self.prop(node, "Name").await?;
        Ok(String::try_from(v).unwrap_or_default())
    }

    async fn prop(&mut self, node: &OwnedObjectPath, name: &str) -> zbus::Result<OwnedValue> {
        self.call(
            node.as_str(),
            "org.freedesktop.DBus.Properties",
            "Get",
            &(ACCESSIBLE, name),
        )
        .await
    }
}

async fn call<B, R>(
    conn: &Connection,
    dest: &str,
    path: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> zbus::Result<R>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    let reply = conn
        .call_method(Some(dest), path, Some(iface), method, body)
        .await?;
    reply.body().deserialize()
}
