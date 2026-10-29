//! Read-only inventory aid: does not display credentials, execute tools, authorize,
//! write configuration, or make network/model requests. Connection summaries may
//! read receipt-owned fields to check whether previous test results still apply.
fn main() -> anyhow::Result<()> {
    for entry in tokensmarket_core::inventory::ToolInventory::current().list()? {
        println!(
            "{}: {}",
            entry.name,
            if entry.installed {
                "installed"
            } else {
                "missing"
            }
        );
    }
    Ok(())
}
