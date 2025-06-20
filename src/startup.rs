use crate::Controller;

/// Core logic that runs during each cycle.
pub async fn run(controller: &mut Controller) -> anyhow::Result<()> {
    controller.fetch_rtt_containers().await?;
    controller.fetch_and_set_start_times().await?;
    controller.sort()?;
    controller.restart_if_needed().await?;
    controller.cleanup_after_cycle();
    Ok(())
}
