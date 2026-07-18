use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskType {
    Discover,
    Scan,
    Monitor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanParams {
    pub domains: Vec<String>,
    pub ports: Vec<u16>,
    pub concurrency: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub found_servers: usize,
    pub discovered: Vec<(String, u16)>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: TaskType,
    pub params: ScanParams,
    pub status: TaskStatus,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub result: Option<TaskResult>,
    pub claimed_by: Option<String>,
}

impl Task {
    pub fn new(task_type: TaskType, params: ScanParams) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            id: Uuid::new_v4().to_string(),
            task_type,
            params,
            status: TaskStatus::Pending,
            created_at: now,
            started_at: None,
            completed_at: None,
            result: None,
            claimed_by: None,
        }
    }

    pub fn start(&mut self, worker_id: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.status = TaskStatus::Running;
        self.started_at = Some(now);
        self.claimed_by = Some(worker_id.to_string());
    }

    pub fn complete(&mut self, result: TaskResult) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.status = TaskStatus::Completed;
        self.completed_at = Some(now);
        self.result = Some(result);
    }

    pub fn fail(&mut self, error: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.status = TaskStatus::Failed;
        self.completed_at = Some(now);
        self.result = Some(TaskResult {
            found_servers: 0,
            discovered: Vec::new(),
            error: Some(error.to_string()),
        });
    }
}

pub struct TaskQueue {
    tasks: BTreeMap<String, Task>,
    pending: VecDeque<String>,
    path: PathBuf,
}

impl TaskQueue {
    pub fn new(path: PathBuf) -> Self {
        let mut q = Self {
            tasks: BTreeMap::new(),
            pending: VecDeque::new(),
            path,
        };
        q.load();
        q
    }

    fn load(&mut self) {
        if !self.path.exists() {
            return;
        }
        match fs::read_to_string(&self.path) {
            Ok(content) => {
                if let Ok(tasks) = serde_json::from_str::<Vec<Task>>(&content) {
                    for task in tasks {
                        self.tasks.insert(task.id.clone(), task);
                    }
                    self.rebuild_pending();
                }
            }
            Err(e) => {
                log::warn!("[tasks] Failed to load queue: {}", e);
            }
        }
    }

    fn save(&self) {
        let tasks: Vec<Task> = self.tasks.values().cloned().collect();
        if let Ok(json) = serde_json::to_string_pretty(&tasks) {
            if let Some(parent) = self.path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&self.path, json);
        }
    }

    fn rebuild_pending(&mut self) {
        self.pending.clear();
        for task in self.tasks.values() {
            if task.status == TaskStatus::Pending {
                self.pending.push_back(task.id.clone());
            }
        }
    }

    pub fn enqueue(&mut self, task: Task) {
        let id = task.id.clone();
        self.tasks.insert(id.clone(), task);
        self.pending.push_back(id);
        self.save();
    }

    pub fn dequeue(&mut self, worker_id: &str) -> Option<Task> {
        while let Some(id) = self.pending.pop_front() {
            if let Some(task) = self.tasks.get_mut(&id) {
                if task.status == TaskStatus::Pending {
                    task.start(worker_id);
                    let cloned = task.clone();
                    self.save();
                    return Some(cloned);
                }
            }
        }
        None
    }

    pub fn update(&mut self, task: Task) {
        let id = task.id.clone();
        let status = task.status.clone();
        self.tasks.insert(id, task);
        if status != TaskStatus::Running {
            self.rebuild_pending();
        }
        self.save();
    }

    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.get(id)
    }

    pub fn list(&self) -> Vec<Task> {
        self.tasks.values().cloned().collect()
    }

    pub fn list_pending(&self) -> Vec<Task> {
        self.pending
            .iter()
            .filter_map(|id| self.tasks.get(id))
            .cloned()
            .collect()
    }

    pub fn remove(&mut self, id: &str) {
        self.tasks.remove(id);
        self.rebuild_pending();
        self.save();
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn total_count(&self) -> usize {
        self.tasks.len()
    }
}

pub fn default_queue_path() -> PathBuf {
    let dir = directories::ProjectDirs::from("", "", "creeper")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| "./data".into());
    dir.join("task-queue.json")
}
