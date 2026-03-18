//! Contact Book — database operations.
//!
//! Extends `Database` with CRUD for contacts, identities, relationships, events.

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{
    Contact, ContactEvent, ContactIdentity, ContactRelationship, PendingResponse, UpcomingEvent,
};
use crate::storage::Database;

// ── Update request ──────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct ContactUpdate {
    pub name: Option<String>,
    pub nickname: Option<String>,
    pub bio: Option<String>,
    pub notes: Option<String>,
    pub birthday: Option<String>,
    pub nameday: Option<String>,
    pub preferred_channel: Option<String>,
    pub response_mode: Option<String>,
    pub tags: Option<String>,
    pub avatar_url: Option<String>,
}

// ── Contacts CRUD ───────────────────────────────────────────────────

impl Database {
    pub async fn insert_contact(
        &self,
        name: &str,
        nickname: Option<&str>,
        bio: Option<&str>,
        notes: Option<&str>,
        birthday: Option<&str>,
        nameday: Option<&str>,
        preferred_channel: Option<&str>,
        response_mode: Option<&str>,
        tags: Option<&str>,
    ) -> Result<i64> {
        let mode = response_mode.unwrap_or("automatic");
        let tags_val = tags.unwrap_or("[]");

        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contacts (name, nickname, bio, notes, birthday, nameday, preferred_channel, response_mode, tags)
             VALUES (?, ?, COALESCE(?, ''), COALESCE(?, ''), ?, ?, ?, ?, ?)
             RETURNING id",
        )
        .bind(name)
        .bind(nickname)
        .bind(bio)
        .bind(notes)
        .bind(birthday)
        .bind(nameday)
        .bind(preferred_channel)
        .bind(mode)
        .bind(tags_val)
        .fetch_one(self.pool())
        .await
        .context("Failed to insert contact")?;

        Ok(id)
    }

    pub async fn load_contact(&self, id: i64) -> Result<Option<Contact>> {
        let row = sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .context("Failed to load contact")?;
        Ok(row)
    }

    pub async fn list_contacts(&self, query: Option<&str>) -> Result<Vec<Contact>> {
        match query {
            Some(q) if !q.is_empty() => {
                let pattern = format!("%{q}%");
                sqlx::query_as::<_, Contact>(
                    "SELECT * FROM contacts
                     WHERE name LIKE ?1 OR nickname LIKE ?1 OR bio LIKE ?1
                     ORDER BY name COLLATE NOCASE",
                )
                .bind(&pattern)
                .fetch_all(self.pool())
                .await
                .context("Failed to list contacts")
            }
            _ => {
                sqlx::query_as::<_, Contact>("SELECT * FROM contacts ORDER BY name COLLATE NOCASE")
                    .fetch_all(self.pool())
                    .await
                    .context("Failed to list contacts")
            }
        }
    }

    pub async fn update_contact(&self, id: i64, upd: &ContactUpdate) -> Result<bool> {
        let mut sets = Vec::new();
        let mut vals: Vec<String> = Vec::new();

        macro_rules! maybe_set {
            ($field:ident) => {
                if let Some(v) = &upd.$field {
                    sets.push(concat!(stringify!($field), " = ?"));
                    vals.push(v.clone());
                }
            };
        }
        maybe_set!(name);
        maybe_set!(nickname);
        maybe_set!(bio);
        maybe_set!(notes);
        maybe_set!(birthday);
        maybe_set!(nameday);
        maybe_set!(preferred_channel);
        maybe_set!(response_mode);
        maybe_set!(tags);
        maybe_set!(avatar_url);

        if sets.is_empty() {
            return Ok(false);
        }

        sets.push("updated_at = datetime('now')");
        let sql = format!("UPDATE contacts SET {} WHERE id = ?", sets.join(", "));

        let mut q = sqlx::query(&sql);
        for v in &vals {
            q = q.bind(v);
        }
        q = q.bind(id);

        let rows = q
            .execute(self.pool())
            .await
            .context("Failed to update contact")?
            .rows_affected();

        Ok(rows > 0)
    }

    pub async fn delete_contact(&self, id: i64) -> Result<bool> {
        let rows = sqlx::query("DELETE FROM contacts WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .context("Failed to delete contact")?
            .rows_affected();
        Ok(rows > 0)
    }
}

// ── Identities ──────────────────────────────────────────────────────

impl Database {
    pub async fn insert_contact_identity(
        &self,
        contact_id: i64,
        channel: &str,
        identifier: &str,
        label: Option<&str>,
    ) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contact_identities (contact_id, channel, identifier, label)
             VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(contact_id)
        .bind(channel)
        .bind(identifier)
        .bind(label)
        .fetch_one(self.pool())
        .await
        .context("Failed to insert contact identity")?;
        Ok(id)
    }

    pub async fn list_contact_identities(&self, contact_id: i64) -> Result<Vec<ContactIdentity>> {
        sqlx::query_as::<_, ContactIdentity>(
            "SELECT * FROM contact_identities WHERE contact_id = ? ORDER BY channel",
        )
        .bind(contact_id)
        .fetch_all(self.pool())
        .await
        .context("Failed to list contact identities")
    }

    pub async fn find_contact_by_identity(
        &self,
        channel: &str,
        identifier: &str,
    ) -> Result<Option<Contact>> {
        sqlx::query_as::<_, Contact>(
            "SELECT c.* FROM contacts c
             JOIN contact_identities ci ON ci.contact_id = c.id
             WHERE ci.channel = ? AND ci.identifier = ?",
        )
        .bind(channel)
        .bind(identifier)
        .fetch_optional(self.pool())
        .await
        .context("Failed to find contact by identity")
    }

    pub async fn delete_contact_identity(&self, id: i64) -> Result<bool> {
        let rows = sqlx::query("DELETE FROM contact_identities WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .context("Failed to delete contact identity")?
            .rows_affected();
        Ok(rows > 0)
    }
}

// ── Relationships ───────────────────────────────────────────────────

impl Database {
    pub async fn insert_contact_relationship(
        &self,
        from_id: i64,
        to_id: i64,
        rel_type: &str,
        bidirectional: bool,
        reverse_type: Option<&str>,
        notes: Option<&str>,
    ) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contact_relationships (from_contact_id, to_contact_id, relationship_type, bidirectional, reverse_type, notes)
             VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(from_id)
        .bind(to_id)
        .bind(rel_type)
        .bind(bidirectional as i32)
        .bind(reverse_type)
        .bind(notes)
        .fetch_one(self.pool())
        .await
        .context("Failed to insert contact relationship")?;
        Ok(id)
    }

    pub async fn list_contact_relationships(
        &self,
        contact_id: i64,
    ) -> Result<Vec<ContactRelationship>> {
        sqlx::query_as::<_, ContactRelationship>(
            "SELECT * FROM contact_relationships
             WHERE from_contact_id = ? OR to_contact_id = ?
             ORDER BY relationship_type",
        )
        .bind(contact_id)
        .bind(contact_id)
        .fetch_all(self.pool())
        .await
        .context("Failed to list contact relationships")
    }

    pub async fn delete_contact_relationship(&self, id: i64) -> Result<bool> {
        let rows = sqlx::query("DELETE FROM contact_relationships WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .context("Failed to delete contact relationship")?
            .rows_affected();
        Ok(rows > 0)
    }
}

// ── Events ──────────────────────────────────────────────────────────

impl Database {
    pub async fn insert_contact_event(
        &self,
        contact_id: i64,
        event_type: &str,
        date: &str,
        recurrence: Option<&str>,
        label: Option<&str>,
        auto_greet: bool,
        notify_days_before: Option<i32>,
    ) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contact_events (contact_id, event_type, date, recurrence, label, auto_greet, notify_days_before)
             VALUES (?, ?, ?, COALESCE(?, 'yearly'), ?, ?, COALESCE(?, 1)) RETURNING id",
        )
        .bind(contact_id)
        .bind(event_type)
        .bind(date)
        .bind(recurrence)
        .bind(label)
        .bind(auto_greet as i32)
        .bind(notify_days_before)
        .fetch_one(self.pool())
        .await
        .context("Failed to insert contact event")?;
        Ok(id)
    }

    pub async fn list_contact_events(&self, contact_id: i64) -> Result<Vec<ContactEvent>> {
        sqlx::query_as::<_, ContactEvent>(
            "SELECT * FROM contact_events WHERE contact_id = ? ORDER BY date",
        )
        .bind(contact_id)
        .fetch_all(self.pool())
        .await
        .context("Failed to list contact events")
    }

    pub async fn delete_contact_event(&self, id: i64) -> Result<bool> {
        let rows = sqlx::query("DELETE FROM contact_events WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .context("Failed to delete contact event")?
            .rows_affected();
        Ok(rows > 0)
    }
}

// ── Upcoming events ─────────────────────────────────────────────────

impl Database {
    /// Load contact events whose MM-DD falls within the next `days` days.
    /// Returns events joined with contact name for display.
    pub async fn load_upcoming_contact_events(&self, days: i32) -> Result<Vec<UpcomingEvent>> {
        // For yearly recurrence: compare MM-DD portion.
        // For 'once': compare full date.
        let rows = sqlx::query_as::<
            _,
            (
                i64,
                i64,
                String,
                String,
                String,
                Option<String>,
                i32,
                Option<String>,
                i32,
                String,
                String,
            ),
        >(
            "SELECT ce.id, ce.contact_id, ce.event_type, ce.date, ce.recurrence,
                    ce.label, ce.auto_greet, ce.greet_template, ce.notify_days_before,
                    ce.created_at, c.name
             FROM contact_events ce
             JOIN contacts c ON c.id = ce.contact_id
             WHERE (ce.recurrence = 'yearly'
                    AND substr(ce.date, -5) BETWEEN strftime('%m-%d', 'now')
                                                AND strftime('%m-%d', 'now', '+' || ? || ' days'))
                OR (ce.recurrence = 'once'
                    AND ce.date BETWEEN date('now') AND date('now', '+' || ? || ' days'))
                OR (ce.recurrence = 'monthly'
                    AND CAST(substr(ce.date, -2) AS INTEGER)
                        BETWEEN CAST(strftime('%d', 'now') AS INTEGER)
                            AND CAST(strftime('%d', 'now', '+' || ? || ' days') AS INTEGER))
             ORDER BY ce.date",
        )
        .bind(days)
        .bind(days)
        .bind(days)
        .fetch_all(self.pool())
        .await
        .context("Failed to load upcoming contact events")?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    contact_id,
                    event_type,
                    date,
                    recurrence,
                    label,
                    auto_greet,
                    greet_template,
                    notify_days_before,
                    created_at,
                    contact_name,
                )| {
                    UpcomingEvent {
                        event: ContactEvent {
                            id,
                            contact_id,
                            event_type,
                            date,
                            recurrence,
                            label,
                            auto_greet,
                            greet_template,
                            notify_days_before,
                            created_at,
                        },
                        contact_name,
                    }
                },
            )
            .collect())
    }
}

// ── Pending responses ───────────────────────────────────────────────

impl Database {
    pub async fn insert_pending_response(
        &self,
        contact_id: Option<i64>,
        channel: &str,
        chat_id: &str,
        inbound_content: &str,
        draft_response: Option<&str>,
    ) -> Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO pending_responses (contact_id, channel, chat_id, inbound_content, draft_response)
             VALUES (?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(contact_id)
        .bind(channel)
        .bind(chat_id)
        .bind(inbound_content)
        .bind(draft_response)
        .fetch_one(self.pool())
        .await
        .context("Failed to insert pending response")?;
        Ok(id)
    }

    pub async fn list_pending_responses(
        &self,
        status: Option<&str>,
    ) -> Result<Vec<PendingResponse>> {
        match status {
            Some(s) => sqlx::query_as::<_, PendingResponse>(
                "SELECT * FROM pending_responses WHERE status = ? ORDER BY created_at DESC",
            )
            .bind(s)
            .fetch_all(self.pool())
            .await
            .context("Failed to list pending responses"),
            None => sqlx::query_as::<_, PendingResponse>(
                "SELECT * FROM pending_responses ORDER BY created_at DESC",
            )
            .fetch_all(self.pool())
            .await
            .context("Failed to list pending responses"),
        }
    }

    pub async fn update_pending_response_status(&self, id: i64, status: &str) -> Result<bool> {
        let rows = sqlx::query("UPDATE pending_responses SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(self.pool())
            .await
            .context("Failed to update pending response status")?
            .rows_affected();
        Ok(rows > 0)
    }

    pub async fn load_pending_response(&self, id: i64) -> Result<Option<PendingResponse>> {
        sqlx::query_as::<_, PendingResponse>("SELECT * FROM pending_responses WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await
            .context("Failed to load pending response")
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_db() -> (Database, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).await.unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn test_contact_crud() {
        let (db, _dir) = test_db().await;

        // Create
        let id = db
            .insert_contact(
                "Marco Rossi",
                Some("marco"),
                Some("CTO"),
                None,
                Some("1990-03-21"),
                None,
                Some("telegram"),
                None,
                None,
            )
            .await
            .unwrap();
        assert!(id > 0);

        // Read
        let c = db.load_contact(id).await.unwrap().unwrap();
        assert_eq!(c.name, "Marco Rossi");
        assert_eq!(c.nickname.as_deref(), Some("marco"));
        assert_eq!(c.bio, "CTO");
        assert_eq!(c.response_mode, "automatic");

        // Update
        let upd = ContactUpdate {
            bio: Some("CTO di AcmeCorp".into()),
            response_mode: Some("assisted".into()),
            ..Default::default()
        };
        assert!(db.update_contact(id, &upd).await.unwrap());
        let c = db.load_contact(id).await.unwrap().unwrap();
        assert_eq!(c.bio, "CTO di AcmeCorp");
        assert_eq!(c.response_mode, "assisted");

        // List
        let all = db.list_contacts(None).await.unwrap();
        assert_eq!(all.len(), 1);

        // Search
        let found = db.list_contacts(Some("marco")).await.unwrap();
        assert_eq!(found.len(), 1);
        let empty = db.list_contacts(Some("zzz")).await.unwrap();
        assert!(empty.is_empty());

        // Delete
        assert!(db.delete_contact(id).await.unwrap());
        assert!(db.load_contact(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_identities() {
        let (db, _dir) = test_db().await;
        let cid = db
            .insert_contact("Test", None, None, None, None, None, None, None, None)
            .await
            .unwrap();

        let iid = db
            .insert_contact_identity(cid, "telegram", "12345", Some("personal"))
            .await
            .unwrap();
        assert!(iid > 0);

        // List
        let ids = db.list_contact_identities(cid).await.unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].channel, "telegram");
        assert_eq!(ids[0].identifier, "12345");

        // Find by identity
        let found = db
            .find_contact_by_identity("telegram", "12345")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, cid);

        // Not found
        assert!(db
            .find_contact_by_identity("telegram", "99999")
            .await
            .unwrap()
            .is_none());

        // Delete identity
        assert!(db.delete_contact_identity(iid).await.unwrap());
        assert!(db.list_contact_identities(cid).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_relationships() {
        let (db, _dir) = test_db().await;
        let a = db
            .insert_contact("Alice", None, None, None, None, None, None, None, None)
            .await
            .unwrap();
        let b = db
            .insert_contact("Bob", None, None, None, None, None, None, None, None)
            .await
            .unwrap();

        let rid = db
            .insert_contact_relationship(a, b, "madre", true, Some("figlio"), None)
            .await
            .unwrap();
        assert!(rid > 0);

        // List from Alice's perspective
        let rels = db.list_contact_relationships(a).await.unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].relationship_type, "madre");

        // Also visible from Bob's perspective
        let rels = db.list_contact_relationships(b).await.unwrap();
        assert_eq!(rels.len(), 1);

        // Delete
        assert!(db.delete_contact_relationship(rid).await.unwrap());
        assert!(db.list_contact_relationships(a).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_events() {
        let (db, _dir) = test_db().await;
        let cid = db
            .insert_contact("Test", None, None, None, None, None, None, None, None)
            .await
            .unwrap();

        let eid = db
            .insert_contact_event(
                cid,
                "birthday",
                "03-21",
                None,
                Some("Compleanno"),
                false,
                Some(2),
            )
            .await
            .unwrap();
        assert!(eid > 0);

        let evs = db.list_contact_events(cid).await.unwrap();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].event_type, "birthday");
        assert_eq!(evs[0].label.as_deref(), Some("Compleanno"));
        assert_eq!(evs[0].notify_days_before, 2);

        assert!(db.delete_contact_event(eid).await.unwrap());
        assert!(db.list_contact_events(cid).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_cascade_delete() {
        let (db, _dir) = test_db().await;
        let cid = db
            .insert_contact("Cascade", None, None, None, None, None, None, None, None)
            .await
            .unwrap();
        db.insert_contact_identity(cid, "email", "a@b.com", None)
            .await
            .unwrap();
        db.insert_contact_event(cid, "birthday", "01-01", None, None, false, None)
            .await
            .unwrap();

        // Delete contact → identities + events cascade
        assert!(db.delete_contact(cid).await.unwrap());
        assert!(db.list_contact_identities(cid).await.unwrap().is_empty());
        assert!(db.list_contact_events(cid).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_pending_responses() {
        let (db, _dir) = test_db().await;
        let cid = db
            .insert_contact("Sender", None, None, None, None, None, None, None, None)
            .await
            .unwrap();

        let pid = db
            .insert_pending_response(Some(cid), "telegram", "chat_1", "Hello!", None)
            .await
            .unwrap();
        assert!(pid > 0);

        // List all
        let all = db.list_pending_responses(None).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, "pending");

        // List by status
        let pending = db.list_pending_responses(Some("pending")).await.unwrap();
        assert_eq!(pending.len(), 1);
        let approved = db.list_pending_responses(Some("approved")).await.unwrap();
        assert!(approved.is_empty());

        // Update status
        assert!(db
            .update_pending_response_status(pid, "approved")
            .await
            .unwrap());
        let r = db.load_pending_response(pid).await.unwrap().unwrap();
        assert_eq!(r.status, "approved");
    }
}
