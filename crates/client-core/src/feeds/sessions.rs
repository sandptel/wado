//! The computer's running sessions (`SessionsRequest` → `Sessions`), as rows: what runs, who
//! leads, how many are in it. Served on control rooms (read-only, shared by every device).

use wado_protocol::SessionSummary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// The daemon it runs on (`<remote id>:<instance>`).
    pub instance: String,
    /// The first app's id or title, or "" for an empty desktop.
    pub apps_first: String,
    /// How many more apps.
    pub apps_more: u32,
    /// The device it is sized for, or "".
    pub lead: String,
    pub participants: u32,
}

pub fn rows(list: &[SessionSummary]) -> Vec<SessionRow> {
    list.iter()
        .map(|s| SessionRow {
            instance: s.instance.clone(),
            apps_first: s.apps.first().cloned().unwrap_or_default(),
            apps_more: s.apps.len().saturating_sub(1) as u32,
            lead: s
                .lead
                .clone()
                .or_else(|| s.viewer.clone())
                .unwrap_or_default(),
            participants: s.participants.len() as u32,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_summary_maps_rows() {
        let list: Vec<SessionSummary> = serde_json::from_str(
            r#"[{"instance":"1:1","started_ms":0,"width":1080,"height":2400,"fps":60,
                 "apps":["firefox","foot","code"],"lead":"Pixel",
                 "participants":[{"id":1,"name":"Pixel","lead":true,"since_ms":0,"link":""},
                                 {"id":2,"name":"Laptop","lead":false,"since_ms":0,"link":""}]},
                {"instance":"1:2","started_ms":0,"width":1,"height":1,"fps":30,"apps":[],
                 "viewer":"Old phone"}]"#,
        )
        .unwrap();
        let r = rows(&list);
        assert_eq!(
            r[0],
            SessionRow {
                instance: "1:1".into(),
                apps_first: "firefox".into(),
                apps_more: 2,
                lead: "Pixel".into(),
                participants: 2,
            }
        );
        assert_eq!(r[1].apps_first, "");
        assert_eq!(r[1].apps_more, 0);
        assert_eq!(
            r[1].lead, "Old phone",
            "an older daemon names only its viewer"
        );
    }
}
