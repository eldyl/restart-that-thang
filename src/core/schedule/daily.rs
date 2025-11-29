use chrono::{DateTime, Local, NaiveDateTime, NaiveTime, Utc};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ScheduleDaily(NaiveTime);

impl ScheduleDaily {
    pub fn new(time: NaiveTime) -> Self {
        Self(time)
    }

    pub fn next_restart_time(&self) -> anyhow::Result<DateTime<Utc>> {
        let now = Local::now();
        let today = now.date_naive();
        let next_local = NaiveDateTime::new(today, *self.as_ref())
            .and_local_timezone(Local)
            .single()
            .unwrap_or_else(|| {
                NaiveDateTime::new(today, *self.as_ref())
                    .and_local_timezone(Local)
                    .earliest()
                    .unwrap()
            });

        let mut next = next_local.with_timezone(&Utc);

        if next <= Utc::now() {
            next += chrono::Duration::days(1);
        }

        Ok(next)
    }
}

impl AsRef<NaiveTime> for ScheduleDaily {
    fn as_ref(&self) -> &NaiveTime {
        &self.0
    }
}
