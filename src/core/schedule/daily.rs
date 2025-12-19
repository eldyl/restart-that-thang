use super::clock::{Clock, SystemClock};
use super::MINUTES_IN_HOUR;
use chrono::{DateTime, Local, LocalResult, NaiveDateTime, NaiveTime, TimeDelta, Timelike, Utc};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ScheduleDaily(NaiveTime);

impl ScheduleDaily {
    pub fn new(time: NaiveTime) -> Self {
        Self(time)
    }

    /// Get the next scheduled restart time from the ScheduleDaily that contains a NaiveTime.
    pub fn next_restart_time(&self) -> anyhow::Result<DateTime<Utc>> {
        self.next_restart_time_impl(&SystemClock)
    }

    // Impl function is used to allow for improved unit testing around getting the next restart time.
    //
    // Implementation considers timezones and daylight savings time.
    //
    // During daylight savings transitions, the behavior is defined as follows:
    // - During spring forward, the missed time will run as soon as possible.
    // - During fall back, we get the earliest available time and use the earliest time as the next
    // restart time.
    fn next_restart_time_impl(&self, clock: &impl Clock) -> anyhow::Result<DateTime<Utc>> {
        let now = clock.now_local();
        let today = now.date_naive();
        let next_local = NaiveDateTime::new(today, *self.as_ref()).and_local_timezone(Local);

        let local_result = match next_local {
            LocalResult::Single(t) => t,
            // There is a fold in local time, take the earliest result available.
            LocalResult::Ambiguous(early, _late) => early,
            LocalResult::None => {
                // When the time does not exist, we create the next time that exists which occurrs
                // on the next hour.

                // Isolate the minutes provided for the instance of ScheduleDaily.
                let minute_in_schedule_daily = self.as_ref().minute();
                // Get the difference from minute_in_schedule_daily to the next rounded hour, which
                // provides the minutes needed to add to the existing time to get the time that is
                // the next hour after minute_in_schedule_daily;
                let minutes_to_add_to_round_to_next_hour =
                    MINUTES_IN_HOUR.saturating_sub(minute_in_schedule_daily);
                let adjusted_time = self.as_ref().overflowing_add_signed(
                    TimeDelta::try_minutes(minutes_to_add_to_round_to_next_hour.into())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Failed to resolve scheduled time {} to a valid local time",
                                self.as_ref()
                            )
                        })?,
                );
                NaiveDateTime::new(today, adjusted_time.0)
                    .and_local_timezone(Local)
                    .single()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Failed to resolve scheduled time {} to a valid local time",
                            self.as_ref()
                        )
                    })?
            }
        };

        let mut next = local_result.with_timezone(&Utc);

        if next <= clock.now_utc() {
            next = next.checked_add_days(chrono::Days::new(1)).ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to resolve scheduled time {} to a valid local time",
                    self.as_ref()
                )
            })?;
        }

        Ok(next)
    }
}

impl AsRef<NaiveTime> for ScheduleDaily {
    fn as_ref(&self) -> &NaiveTime {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Local, NaiveTime, TimeZone, Utc};

    struct MockClock {
        pub fixed_local: DateTime<Local>,
        pub fixed_utc: DateTime<Utc>,
    }

    impl MockClock {
        pub fn new(year: i32, month: u32, day: u32, hour: u32, min: u32) -> Self {
            let local_result = Local.with_ymd_and_hms(year, month, day, hour, min, 0);
            let local = match local_result {
                chrono::LocalResult::Single(dt) => dt,
                chrono::LocalResult::Ambiguous(early, _late) => early,
                chrono::LocalResult::None => Local
                    .with_ymd_and_hms(year, month, day, hour, min, 0)
                    .single()
                    .expect("Failed to create MockClock for non-existent time"),
            };
            let utc = local.with_timezone(&Utc);
            Self {
                fixed_local: local,
                fixed_utc: utc,
            }
        }
    }

    impl Clock for MockClock {
        fn now_local(&self) -> DateTime<Local> {
            self.fixed_local
        }

        fn now_utc(&self) -> DateTime<Utc> {
            self.fixed_utc
        }
    }

    #[test]
    fn test_scheduled_for_today_time_is_in_future() {
        let clock = MockClock::new(2025, 1, 15, 10, 0);

        let schedule_time = NaiveTime::from_hms_opt(14, 0, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 1, 15, 14, 0, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    #[test]
    fn test_scheduled_for_tomorrow_time_is_in_past() {
        let clock = MockClock::new(2025, 1, 15, 10, 0);

        let schedule_time = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 1, 16, 9, 0, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    #[test]
    fn test_scheduled_for_tomorrow_time_is_now() {
        let clock = MockClock::new(2025, 1, 15, 12, 0);

        let schedule_time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 1, 16, 12, 0, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    #[test]
    fn test_scheduled_for_tomorrow_crossing_midnight() {
        let clock = MockClock::new(2025, 1, 15, 23, 0);

        let schedule_time = NaiveTime::from_hms_opt(1, 0, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 1, 16, 1, 0, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    #[test]
    fn test_scheduled_for_tomorrow_crossing_new_month() {
        let clock = MockClock::new(2025, 11, 30, 23, 0);

        let schedule_time = NaiveTime::from_hms_opt(22, 0, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 12, 1, 22, 0, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    #[test]
    fn test_scheduled_for_tomorrow_crossing_new_year() {
        let clock = MockClock::new(2025, 12, 31, 23, 0);

        let schedule_time = NaiveTime::from_hms_opt(22, 0, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2026, 1, 1, 22, 0, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    // *************************************************************************
    // Testing for edge cases around Daylight Savings
    //
    // Spring forward
    // Time goes forward. At 0200 time becomes 0300.
    //
    // Fall back
    // Time goes backward. At 0200 time becomes 0100.
    //
    // One timezone exists where daylight savings only changes the time by 30 minutes. The
    // offending timezone is 'Australia/Lord_Howe'.
    //
    // Note: In the tests below, 'ds' is short for 'daylight savings', 'sf' is short for 'spring
    // forward', and 'fb' is short for 'fall back'.
    //
    // *************************************************************************

    // A scheduled time of 0130 should not be impacted by spring forward.
    #[test]
    fn test_ds_sf_0130_should_be_unaffected() {
        let clock = MockClock::new(2025, 3, 9, 0, 0);

        let schedule_time = NaiveTime::from_hms_opt(1, 30, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 3, 9, 1, 30, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    // A scheduled time of 0200, an hour which will not exist, should return a time of 0300. The
    // determined behavior is to return the earliest available time that exists.
    #[test]
    fn test_ds_sf_0200_should_be_0300() {
        let clock = MockClock::new(2025, 3, 9, 0, 0);

        let schedule_time = NaiveTime::from_hms_opt(2, 0, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 3, 9, 3, 00, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    // A scheduled time of 0230, an hour which will not exist, should return a time of 0300. The
    // determined behavior is to return the earliest available time that exists.
    #[test]
    fn test_ds_sf_0230_should_be_0300() {
        let clock = MockClock::new(2025, 3, 9, 0, 0);

        let schedule_time = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 3, 9, 3, 00, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    // A scheduled time of 0201, an hour which will not exist, should return a time of 0300. The
    // determined behavior is to return the earliest available time that exists.
    #[test]
    fn test_ds_sf_0201_should_be_0300() {
        let clock = MockClock::new(2025, 3, 9, 0, 0);

        let schedule_time = NaiveTime::from_hms_opt(2, 1, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 3, 9, 3, 00, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    // A scheduled time of 0259, an hour which will not exist, should return a time of 0300. The
    // determined behavior is to return the earliest available time that exists.
    #[test]
    fn test_ds_sf_0259_should_be_0300() {
        let clock = MockClock::new(2025, 3, 9, 0, 0);

        let schedule_time = NaiveTime::from_hms_opt(2, 59, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 3, 9, 3, 00, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    #[test]
    fn test_ds_sf_0300_should_stay_0300() {
        let clock = MockClock::new(2025, 3, 9, 0, 0);

        let schedule_time = NaiveTime::from_hms_opt(3, 00, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 3, 9, 3, 00, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    #[test]
    fn test_ds_sf_day_after_0300_should_be_0300_again() {
        let clock = MockClock::new(2025, 3, 10, 0, 0);

        let schedule_time = NaiveTime::from_hms_opt(3, 00, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local.with_ymd_and_hms(2025, 3, 10, 3, 00, 0).unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    #[test]
    fn test_ds_fb_0130_is_first_occurring_0130() {
        let clock = MockClock::new(2025, 11, 2, 1, 20);

        let schedule_time = NaiveTime::from_hms_opt(1, 30, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local
            .with_ymd_and_hms(2025, 11, 2, 1, 30, 0)
            .earliest()
            .unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }

    #[test]
    fn test_ds_fb_after_first_0130_returns_0130_tomorrow() {
        let clock = MockClock::new(2025, 11, 2, 1, 40);

        let schedule_time = NaiveTime::from_hms_opt(1, 30, 0).unwrap();
        let schedule = ScheduleDaily::new(schedule_time);

        let result = schedule.next_restart_time_impl(&clock).unwrap();

        let expected_local = Local
            .with_ymd_and_hms(2025, 11, 3, 1, 30, 0)
            .earliest()
            .unwrap();
        let expected_utc = expected_local.with_timezone(&Utc);

        assert_eq!(result, expected_utc);
    }
}
