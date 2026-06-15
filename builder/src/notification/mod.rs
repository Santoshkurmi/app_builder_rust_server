pub mod handlers;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledNotification {
    pub id: String,
    pub key: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub run_at: chrono::DateTime<chrono::Utc>,
    pub data: serde_json::Value,
    #[serde(default)]
    pub attempts: u32,
}

#[derive(Clone)]
pub struct ActiveNotification {
    pub item: ScheduledNotification,
    pub cancel_token: CancellationToken,
}

#[derive(Clone)]
pub struct NotificationManager {
    pub callback_url: Option<String>,
    pub persistence_path: PathBuf,
    pub delete_only_on_success: bool,
    pub retry_count: u32,
    pub retry_interval: u64,
    pub active_notifications: Arc<Mutex<HashMap<String, HashMap<String, ActiveNotification>>>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ScheduleRequest {
    #[serde(default = "default_action")]
    pub action: String, // "add", "update", "update_force", "delete"
    pub key: String,    // Group key
    #[serde(default)]
    pub items: Vec<NotificationItem>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NotificationItem {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub r#type: Option<String>,
    pub run_at: Option<chrono::DateTime<chrono::Utc>>,
    pub data: Option<serde_json::Value>,
}

fn default_action() -> String {
    "add".to_string()
}

impl NotificationManager {
    pub fn new(
        callback_url: Option<String>,
        persistence_path: PathBuf,
        delete_only_on_success: bool,
        retry_count: u32,
        retry_interval: u64,
    ) -> Self {
        Self {
            callback_url,
            persistence_path,
            delete_only_on_success,
            retry_count,
            retry_interval,
            active_notifications: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn save_to_disk(&self, active: &HashMap<String, HashMap<String, ActiveNotification>>) {
        let mut list = Vec::new();
        for group in active.values() {
            for active_notif in group.values() {
                list.push(active_notif.item.clone());
            }
        }

        if let Some(parent) = self.persistence_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match serde_json::to_string_pretty(&list) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.persistence_path, json) {
                    log::error!("Failed to write scheduled notifications to disk: {:?}", e);
                }
            }
            Err(e) => {
                log::error!("Failed to serialize scheduled notifications: {:?}", e);
            }
        }
    }

    pub fn load_from_disk(&self) -> Vec<ScheduledNotification> {
        if !self.persistence_path.exists() {
            return Vec::new();
        }

        match std::fs::read_to_string(&self.persistence_path) {
            Ok(content) => {
                match serde_json::from_str::<Vec<ScheduledNotification>>(&content) {
                    Ok(list) => list,
                    Err(e) => {
                        log::error!("Failed to deserialize scheduled notifications: {:?}", e);
                        Vec::new()
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to read scheduled notifications from disk: {:?}", e);
                Vec::new()
            }
        }
    }

    pub async fn load_and_reschedule(&self) {
        let loaded = self.load_from_disk();
        log::info!("Loaded {} scheduled notifications from disk", loaded.len());

        for item in loaded {
            let cancel_token = CancellationToken::new();
            let cancel_token_clone = cancel_token.clone();
            let key = item.key.clone();
            let id = item.id.clone();

            {
                let mut active = self.active_notifications.lock().await;
                let group = active.entry(key.clone()).or_insert_with(HashMap::new);
                group.insert(id.clone(), ActiveNotification {
                    item: item.clone(),
                    cancel_token,
                });
            }

            let manager = self.clone();
            tokio::spawn(async move {
                let now = chrono::Utc::now();
                let delay = if item.run_at > now {
                    (item.run_at - now).to_std().unwrap_or(std::time::Duration::ZERO)
                } else {
                    std::time::Duration::ZERO
                };

                tokio::select! {
                    _ = tokio::time::sleep(delay) => {
                        manager.dispatch(item).await;
                    }
                    _ = cancel_token_clone.cancelled() => {
                        log::info!("Rescheduled notification task {}/{} cancelled", key, id);
                    }
                }
            });
        }
    }

    pub async fn cancel(&self, key: String, id: String) -> bool {
        let mut active = self.active_notifications.lock().await;
        let mut removed = false;
        if let Some(group) = active.get_mut(&key) {
            if let Some(active_notif) = group.remove(&id) {
                active_notif.cancel_token.cancel();
                removed = true;
            }
            if group.is_empty() {
                active.remove(&key);
            }
        }
        if removed {
            self.save_to_disk(&active);
        }
        removed
    }

    pub async fn cancel_group(&self, key: String) {
        let mut active = self.active_notifications.lock().await;
        if let Some(group) = active.remove(&key) {
            for active_notif in group.values() {
                active_notif.cancel_token.cancel();
            }
            self.save_to_disk(&active);
        }
    }

    pub fn schedule(&self, item: ScheduledNotification) {
        let manager = self.clone();
        tokio::spawn(async move {
            manager.cancel(item.key.clone(), item.id.clone()).await;

            let cancel_token = CancellationToken::new();
            let cancel_token_clone = cancel_token.clone();
            let key = item.key.clone();
            let id = item.id.clone();

            {
                let mut active = manager.active_notifications.lock().await;
                let group = active.entry(key.clone()).or_insert_with(HashMap::new);
                group.insert(id.clone(), ActiveNotification {
                    item: item.clone(),
                    cancel_token,
                });
                manager.save_to_disk(&active);
            }

            let now = chrono::Utc::now();
            let delay = if item.run_at > now {
                (item.run_at - now).to_std().unwrap_or(std::time::Duration::ZERO)
            } else {
                std::time::Duration::ZERO
            };

            tokio::select! {
                _ = tokio::time::sleep(delay) => {
                    manager.dispatch(item).await;
                }
                _ = cancel_token_clone.cancelled() => {
                    log::info!("Notification task {}/{} cancelled", key, id);
                }
            }
        });
    }

    pub async fn dispatch(&self, mut item: ScheduledNotification) {
        let mut dispatch_success = false;

        if let Some(url) = &self.callback_url {
            let client = reqwest::Client::new();
            log::info!("Dispatching notification {}/{} (attempt {}) to callback {}", item.key, item.id, item.attempts + 1, url);
            match client.post(url).json(&item).send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        log::info!("Successfully dispatched notification {}/{}", item.key, item.id);
                        dispatch_success = true;
                    } else {
                        log::error!("Failed to dispatch notification {}/{}: HTTP status {}", item.key, item.id, resp.status());
                    }
                }
                Err(e) => {
                    log::error!("Failed to dispatch notification {}/{}: {:?}", item.key, item.id, e);
                }
            }
        } else {
            log::warn!("No notification callback URL configured. Discarding dispatch for {}/{}", item.key, item.id);
            dispatch_success = true;
        }

        let should_delete = if self.delete_only_on_success {
            dispatch_success
        } else {
            true
        };

        if should_delete {
            let mut active = self.active_notifications.lock().await;
            let mut removed = false;
            if let Some(group) = active.get_mut(&item.key) {
                if group.remove(&item.id).is_some() {
                    removed = true;
                }
                if group.is_empty() {
                    active.remove(&item.key);
                }
            }
            if removed {
                self.save_to_disk(&active);
            }
        } else {
            item.attempts += 1;
            if item.attempts < self.retry_count {
                log::info!("Scheduling retry for notification {}/{} in {} seconds (attempt {} of {})",
                    item.key, item.id, self.retry_interval, item.attempts, self.retry_count);
                item.run_at = chrono::Utc::now() + chrono::Duration::seconds(self.retry_interval as i64);
                self.schedule(item);
            } else {
                log::warn!("Max retry attempts ({}) reached for notification {}/{}. Deleting it.", self.retry_count, item.key, item.id);
                let mut active = self.active_notifications.lock().await;
                let mut removed = false;
                if let Some(group) = active.get_mut(&item.key) {
                    if group.remove(&item.id).is_some() {
                        removed = true;
                    }
                    if group.is_empty() {
                        active.remove(&item.key);
                    }
                }
                if removed {
                    self.save_to_disk(&active);
                }
            }
        }
    }

    pub async fn process_request(&self, req: ScheduleRequest) -> Result<Vec<String>, String> {
        let action = req.action.to_lowercase();
        let key = req.key.trim();
        if key.is_empty() {
            return Err("Group key is required".to_string());
        }

        match action.as_str() {
            "delete" => {
                self.cancel_group(key.to_string()).await;
                Ok(vec![format!("Cancelled all notifications under key {}", key)])
            }
            "add" => {
                let mut results = Vec::new();
                for api_item in req.items {
                    let id = api_item.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let run_at = api_item.run_at.ok_or_else(|| "run_at is required for add".to_string())?;
                    let data = api_item.data.ok_or_else(|| "data is required for add".to_string())?;

                    let item = ScheduledNotification {
                        id,
                        key: key.to_string(),
                        r#type: api_item.r#type.unwrap_or_else(|| "default".to_string()),
                        run_at,
                        data,
                        attempts: 0,
                    };
                    results.push(item.id.clone());
                    self.schedule(item);
                }
                Ok(results)
            }
            "update" => {
                let mut results = Vec::new();
                for api_item in req.items {
                    let id = api_item.id.ok_or_else(|| "id is required for update".to_string())?;
                    let run_at = api_item.run_at.ok_or_else(|| "run_at is required for update".to_string())?;
                    let data = api_item.data.ok_or_else(|| "data is required for update".to_string())?;

                    let item = ScheduledNotification {
                        id,
                        key: key.to_string(),
                        r#type: api_item.r#type.unwrap_or_else(|| "default".to_string()),
                        run_at,
                        data,
                        attempts: 0,
                    };
                    results.push(item.id.clone());
                    self.schedule(item);
                }
                Ok(results)
            }
            "update_force" => {
                let mut incoming_ids = std::collections::HashSet::new();
                let mut items_to_schedule = Vec::new();

                for api_item in req.items {
                    let id = api_item.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let run_at = api_item.run_at.ok_or_else(|| "run_at is required for update_force".to_string())?;
                    let data = api_item.data.ok_or_else(|| "data is required for update_force".to_string())?;

                    incoming_ids.insert(id.clone());
                    items_to_schedule.push(ScheduledNotification {
                        id,
                        key: key.to_string(),
                        r#type: api_item.r#type.unwrap_or_else(|| "default".to_string()),
                        run_at,
                        data,
                        attempts: 0,
                    });
                }

                {
                    let mut active = self.active_notifications.lock().await;
                    if let Some(group) = active.get_mut(key) {
                        let to_remove: Vec<String> = group.keys()
                            .filter(|id| !incoming_ids.contains(*id))
                            .cloned()
                            .collect();

                        for id in to_remove {
                            if let Some(active_notif) = group.remove(&id) {
                                active_notif.cancel_token.cancel();
                            }
                        }
                    }
                    self.save_to_disk(&active);
                }

                let mut results = Vec::new();
                for item in items_to_schedule {
                    results.push(item.id.clone());
                    self.schedule(item);
                }

                Ok(results)
            }
            _ => Err(format!("Unknown action: {}", action)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use chrono::Utc;

    fn assert_send_sync<T: Send + Sync>() {}

    #[tokio::test]
    async fn test_notification_scheduler() {
        assert_send_sync::<NotificationManager>();
        let temp_dir = std::env::temp_dir();
        let log_path = temp_dir.join(format!("notif_test_{}", uuid::Uuid::new_v4()));
        let _ = fs::create_dir_all(&log_path);
        let persistence_path = log_path.join("scheduled_notifications.json");

        let manager = NotificationManager::new(None, persistence_path, false, 3, 60);

        // 1. Test Add
        let req = ScheduleRequest {
            action: "add".to_string(),
            key: "campaign_1".to_string(),
            items: vec![
                NotificationItem {
                    id: Some("item_1".to_string()),
                    r#type: None,
                    run_at: Some(Utc::now() + chrono::Duration::seconds(5)),
                    data: Some(serde_json::json!({"msg": "hello 1"})),
                },
                NotificationItem {
                    id: Some("item_2".to_string()),
                    r#type: None,
                    run_at: Some(Utc::now() + chrono::Duration::seconds(10)),
                    data: Some(serde_json::json!({"msg": "hello 2"})),
                },
            ],
        };

        let result = manager.process_request(req).await.unwrap();
        assert_eq!(result, vec!["item_1".to_string(), "item_2".to_string()]);

        // Sleep briefly to let background spawn task run
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Check in-memory state
        {
            let active = manager.active_notifications.lock().await;
            let group = active.get("campaign_1").unwrap();
            assert_eq!(group.len(), 2);
            assert_eq!(group.get("item_1").unwrap().item.r#type, "default");
            assert_eq!(group.get("item_2").unwrap().item.r#type, "default");
        }

        // Verify disk persistence
        let loaded = manager.load_from_disk();
        assert_eq!(loaded.len(), 2);

        // 2. Test Update (leaves item_2 untouched, updates item_1)
        let req_update = ScheduleRequest {
            action: "update".to_string(),
            key: "campaign_1".to_string(),
            items: vec![
                NotificationItem {
                    id: Some("item_1".to_string()),
                    r#type: Some("custom_type".to_string()),
                    run_at: Some(Utc::now() + chrono::Duration::seconds(20)),
                    data: Some(serde_json::json!({"msg": "hello 1 updated"})),
                }
            ],
        };
        let result_update = manager.process_request(req_update).await.unwrap();
        assert_eq!(result_update, vec!["item_1".to_string()]);

        // Sleep briefly to let background spawn task run
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        {
            let active = manager.active_notifications.lock().await;
            let group = active.get("campaign_1").unwrap();
            assert_eq!(group.len(), 2);
            assert_eq!(group.get("item_1").unwrap().item.data["msg"], "hello 1 updated");
            assert_eq!(group.get("item_1").unwrap().item.r#type, "custom_type");
            // item_2 should be unchanged
            assert_eq!(group.get("item_2").unwrap().item.data["msg"], "hello 2");
            assert_eq!(group.get("item_2").unwrap().item.r#type, "default");
        }

        // 3. Test Update Force (updates item_1, adds item_3, deletes item_2)
        let req_force = ScheduleRequest {
            action: "update_force".to_string(),
            key: "campaign_1".to_string(),
            items: vec![
                NotificationItem {
                    id: Some("item_1".to_string()),
                    r#type: None,
                    run_at: Some(Utc::now() + chrono::Duration::seconds(30)),
                    data: Some(serde_json::json!({"msg": "hello 1 updated force"})),
                },
                NotificationItem {
                    id: Some("item_3".to_string()),
                    r#type: None,
                    run_at: Some(Utc::now() + chrono::Duration::seconds(40)),
                    data: Some(serde_json::json!({"msg": "hello 3"})),
                },
            ],
        };
        let result_force = manager.process_request(req_force).await.unwrap();
        assert_eq!(result_force, vec!["item_1".to_string(), "item_3".to_string()]);

        // Sleep briefly to let background spawn task run
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        {
            let active = manager.active_notifications.lock().await;
            let group = active.get("campaign_1").unwrap();
            assert_eq!(group.len(), 2);
            assert!(group.contains_key("item_1"));
            assert!(!group.contains_key("item_2")); // item_2 should be deleted
            assert!(group.contains_key("item_3"));
        }

        // 4. Test Delete Group
        let req_delete = ScheduleRequest {
            action: "delete".to_string(),
            key: "campaign_1".to_string(),
            items: vec![],
        };
        manager.process_request(req_delete).await.unwrap();

        {
            let active = manager.active_notifications.lock().await;
            assert!(!active.contains_key("campaign_1"));
        }

        // Clean up log path
        let _ = fs::remove_dir_all(log_path);
    }

    #[tokio::test]
    async fn test_notification_retry() {
        let temp_dir = std::env::temp_dir();
        let log_path = temp_dir.join(format!("notif_test_{}", uuid::Uuid::new_v4()));
        let _ = fs::create_dir_all(&log_path);
        let persistence_path = log_path.join("scheduled_notifications.json");

        // Manager set up with delete_only_on_success = true
        let manager = NotificationManager::new(
            Some("http://invalid-url-for-test.test".to_string()),
            persistence_path,
            true, // delete_only_on_success
            3,    // retry_count
            5,    // retry_interval (5 seconds for quick testing)
        );

        let item = ScheduledNotification {
            id: "retry_item".to_string(),
            key: "campaign_retry".to_string(),
            r#type: "default".to_string(),
            run_at: Utc::now() - chrono::Duration::seconds(5), // past timestamp so it executes immediately
            data: serde_json::json!({"msg": "test retry"}),
            attempts: 0,
        };

        // Schedule it, it will execute dispatch immediately because it is in the past
        manager.schedule(item);

        // Sleep briefly to let the tokio task start, fail and reschedule
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Verify that the notification is rescheduled (rescheduled in the queue and attempts incremented)
        let active = manager.active_notifications.lock().await;
        let group = active.get("campaign_retry").expect("Should still have the group since it failed and is retrying");
        let active_notif = group.get("retry_item").expect("Should still have the item");
        
        assert_eq!(active_notif.item.attempts, 1);
        assert!(active_notif.item.run_at > Utc::now()); // run_at has been pushed to the future

        // Clean up files
        let _ = fs::remove_dir_all(log_path);
    }
}
