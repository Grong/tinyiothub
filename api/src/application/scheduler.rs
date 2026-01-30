use std::str::FromStr;

use std::sync::mpsc::{channel, Sender};

use std::sync::{Arc, Weak};

use std::thread;

use std::time::Duration;

use cron::Schedule;

use moka::sync::Cache;

use super::{data_context::DataContext, data_server::DataServer};

use crate::dto::entity::job::Job;

#[derive(Clone)]

struct JobSchedule {
    pub job: Job,

    pub schedule: Schedule,
}

pub struct TimeTask {
    jobs: Cache<String, JobSchedule>,

    data_context: Arc<DataContext>,

    data_server: Weak<DataServer>,

    sender: Sender<TaskEvt>,
}

impl TimeTask {
    pub fn new(dc: Arc<DataContext>, data_server: &Arc<DataServer>) -> Self {
        let map: Cache<String, JobSchedule> = Cache::new(100);

        let list = Job::read(None);

        if let Ok(jobs) = list {
            for job in jobs {
                Self::insert_data(&map, job);
            }
        }

        let (sender, receiver) = channel();

        let hm = map.clone();

        thread::spawn(move || {
            for event in receiver.iter() {
                match event {
                    TaskEvt::Create(j) => {
                        Self::insert_data(&hm, j);
                    }

                    TaskEvt::Update(j) => {
                        hm.invalidate(&j.id.clone());

                        Self::insert_data(&hm, j);
                    }

                    TaskEvt::Delete(j) => {
                        hm.invalidate(&j);
                    }
                }
            }
        });

        Self {
            jobs: map,

            data_context: dc,

            data_server: Arc::downgrade(data_server),

            sender,
        }
    }

    pub fn run(&self) {
        let jobs = self.jobs.clone();

        let ds = self.data_server.upgrade();

        let _data_context = self.data_context.clone();

        thread::spawn(move || loop {
            let now = chrono::Local::now();

            for (_, v) in jobs.iter() {
                if v.schedule.clone().includes(now) && v.job.is_enabled == 1 {
                    if let Some(_ds) = ds.clone() {
                        // Note: Job fields have been updated to match new schema
                        // These fields don't exist in the new Job structure
                        // Commenting out for now to avoid compilation errors

                        // let dev = data_context.get_device_by_name(&v.job.target_device_id);
                        // if let Some(dv) = dev {
                        //     ds.execute_cmd(DeviceCommand {
                        //         id: uuid::Uuid::new_v4().to_string(),
                        //         device_id: dv.id,
                        //         name: v.job.target_command_name.clone(),
                        //         description: None,
                        //         parameters: v.job.target_command_params.clone(),
                        //         created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        //     });
                        // }
                    }
                }
            }

            thread::sleep(Duration::from_secs(1));
        });
    }

    fn insert_data(map: &Cache<String, JobSchedule>, job: Job) {
        let sche = Schedule::from_str(&job.cron_expression);

        match sche {
            Ok(sch) => {
                map.insert(
                    job.id.clone(),
                    JobSchedule {
                        job: job.clone(),

                        schedule: sch,
                    },
                );
            }

            Err(e) => tracing::error!("{e}"),
        }
    }

    pub fn add_job(&self, job: Job) {
        self.sender.send(TaskEvt::Create(job)).unwrap();
    }

    pub fn upd_job(&self, job: Job) {
        self.sender.send(TaskEvt::Update(job)).unwrap();
    }

    pub fn del_job(&self, id: String) {
        self.sender.send(TaskEvt::Delete(id)).unwrap();
    }
}

unsafe impl Send for TimeTask {}

unsafe impl Sync for TimeTask {}

impl Clone for TimeTask {
    fn clone(&self) -> Self {
        Self {
            jobs: self.jobs.clone(),

            data_context: self.data_context.clone(),

            data_server: self.data_server.clone(),

            sender: self.sender.clone(),
        }
    }
}

enum TaskEvt {
    Create(Job),

    Update(Job),

    Delete(String),
}
