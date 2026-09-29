//! Native menu sheets (M-P S7): the menu open in an app, read from its accessibility tree.
//!
//! One job: turn "a popup is open on the focused window" (`wado_compositor::hit::MenuSpot`)
//! into the rows the phone draws as a bottom sheet, and activate the row picked. Tree access
//! is `crate::a11y`'s; this module only knows what a menu looks like in it.
//!
//! A menu is found as the *showing* node with a menu role; its rows are the showing menu-item
//! descendants. The search skips hidden subtrees (state SHOWING), which is what keeps it short
//! in an app whose main view holds hundreds of elements.

use std::time::Duration;

use tracing::debug;
use wado_compositor::hit::MenuSpot;
use wado_protocol::{MenuItem, MenuSheet};
use zbus::zvariant::OwnedObjectPath;

use crate::a11y::{A11y, Query, state};

const MENU_ROLES: &[&str] = &["menu", "popup menu"];
const ITEM_ROLES: &[&str] = &["menu item", "check menu item", "radio menu item"];
/// A menu sheet is read once per popup, not per tap, so it may look further than a tap query.
const CALL_BUDGET: usize = 2500;
const DEADLINE: Duration = Duration::from_millis(600);
const MAX_CHILDREN: i32 = 300;

/// The sheet for the open menu. `tree: false` when the app exposes none (or it could not be
/// read in time): the client then shows the lens over the real popup instead.
pub async fn read(a11y: &A11y, spot: &MenuSpot) -> MenuSheet {
    let (ox, oy) = spot.window.origin;
    let (ow, oh) = spot.window.output;
    let f = spot.window.factor;
    let (x, y, w, h) = spot.rect;
    let mut sheet = MenuSheet {
        items: Vec::new(),
        tree: false,
        x: (ox + x * f) / ow,
        y: (oy + y * f) / oh,
        w: w * f / ow,
        h: h * f / oh,
    };
    match tokio::time::timeout(DEADLINE, items(a11y, spot)).await {
        Ok(Ok(Some(items))) if !items.is_empty() => {
            sheet.items = items;
            sheet.tree = true;
        }
        Ok(Err(e)) => debug!("menu read failed: {e}"),
        Err(_) => debug!("menu read timed out"),
        _ => debug!("no menu items found in the tree"),
    }
    sheet
}

async fn items(a11y: &A11y, spot: &MenuSpot) -> zbus::Result<Option<Vec<MenuItem>>> {
    let Some((conn, dest)) = a11y.app(&spot.window).await? else {
        return Ok(None);
    };
    let mut q = Query {
        conn: &conn,
        dest: &dest,
        calls: 0,
    };
    let Some(frame) = q.frame(&spot.window.title).await? else {
        return Ok(None);
    };
    // Breadth-first over showing nodes until a menu turns up; then its showing items.
    let mut queue = std::collections::VecDeque::from([frame]);
    let mut menus = Vec::new();
    while let Some(node) = queue.pop_front() {
        let n = q.child_count(&node).await?.min(MAX_CHILDREN);
        for i in 0..n {
            if q.calls > CALL_BUDGET {
                debug!("menu search hit its call budget");
                break;
            }
            let child = q.child_at(&node, i).await?;
            if !state::has(&q.states(&child).await?, state::SHOWING) {
                continue;
            }
            if MENU_ROLES.contains(&q.role(&child).await?.as_str()) {
                menus.push(child);
            } else {
                queue.push_back(child);
            }
        }
        if !menus.is_empty() {
            break;
        }
    }
    let mut out = Vec::new();
    for menu in menus {
        collect(&mut q, &menu, &mut out).await?;
    }
    debug!(items = out.len(), calls = q.calls, "menu read");
    Ok(Some(out))
}

/// The showing menu items under `node`, in document order — nested groups and separators
/// flattened. Recursive (boxed): order is the point, and a menu is only a few levels deep.
fn collect<'a, 'q>(
    q: &'a mut Query<'q>,
    node: &'a OwnedObjectPath,
    out: &'a mut Vec<MenuItem>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = zbus::Result<()>> + Send + 'a>>
where
    'q: 'a,
{
    Box::pin(async move {
        let n = q.child_count(node).await?.min(MAX_CHILDREN);
        for i in 0..n {
            if q.calls > CALL_BUDGET {
                return Ok(());
            }
            let child = q.child_at(node, i).await?;
            let st = q.states(&child).await?;
            if !state::has(&st, state::SHOWING) {
                continue;
            }
            let role = q.role(&child).await?;
            if ITEM_ROLES.contains(&role.as_str()) {
                let mut name = q.name(&child).await.unwrap_or_default();
                if name.is_empty() {
                    name = label_of(q, &child).await.unwrap_or_default();
                }
                out.push(MenuItem {
                    id: format!("{}{}", q.dest, child.as_str()),
                    name,
                    enabled: state::has(&st, state::SENSITIVE) || state::has(&st, state::ENABLED),
                    checked: state::has(&st, state::CHECKED),
                    submenu: state::has(&st, state::HAS_POPUP),
                });
            } else {
                collect(q, &child, out).await?;
            }
        }
        Ok(())
    })
}

/// The words of an item that has no name of its own — GTK4 leaves its menu items unnamed
/// (measured on nautilus's context menu, 2026-09-29).
async fn label_of(q: &mut Query<'_>, node: &OwnedObjectPath) -> zbus::Result<String> {
    // GTK4 names it through a relation: LABELLED_BY → a label whose *Text* is the words (it has
    // no Name either). Measured: "Open" came back only this way.
    const LABELLED_BY: u32 = 2;
    for (kind, targets) in q.relations(node).await.unwrap_or_default() {
        if kind != LABELLED_BY {
            continue;
        }
        for (_, label) in targets {
            let name = q.name(&label).await.unwrap_or_default();
            if !name.is_empty() {
                return Ok(name);
            }
            if let Some(t) = q.text(&label).await {
                return Ok(t);
            }
        }
    }
    // Otherwise the item's own text, then its descendants' names or text.
    if let Some(t) = q.text(node).await {
        return Ok(t);
    }
    let mut queue = std::collections::VecDeque::from([(node.clone(), 0)]);
    while let Some((n, depth)) = queue.pop_front() {
        for i in 0..q.child_count(&n).await?.min(8) {
            let c = q.child_at(&n, i).await?;
            let name = q.name(&c).await.unwrap_or_default();
            if !name.is_empty() {
                return Ok(name);
            }
            if let Some(t) = q.text(&c).await {
                return Ok(t);
            }
            if depth < 3 {
                queue.push_back((c, depth + 1));
            }
        }
    }
    Ok(String::new())
}

/// Activate the row `id` (`<bus name><object path>`, as `read` built it).
pub async fn activate(a11y: &A11y, spot: &MenuSpot, id: &str) -> bool {
    let Some(split) = id.find('/') else {
        return false;
    };
    let (dest, path) = id.split_at(split);
    let Ok(Some((conn, _))) = a11y.app(&spot.window).await else {
        return false;
    };
    let mut q = Query {
        conn: &conn,
        dest,
        calls: 0,
    };
    q.activate(path).await.unwrap_or(false)
}
