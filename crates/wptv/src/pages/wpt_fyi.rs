use dioxus::prelude::*;
use crate::wpt_fyi_api::{get_latest_runs, get_run_data};

#[component]
pub fn WptFyiPage() -> Element {
    let runs = use_resource(move || async move {
        let runs = get_latest_runs().await?;
        let run_ids : Vec<_> = runs.iter().map(|run| run.id).collect();
        dbg!(&run_ids);
        let results = get_run_data(&run_ids).await;
        results
    });

    rsx! {
        if let Some(res) = &*runs.read() {
            match res {
                Ok(res) => {
                    let run_count = res.len();
                    rsx! {
                        div {
                            "{run_count}"
                        }
                    }
                },
                Err(err) => rsx! { "Failed to fetch response: {err}" },
            }
        } else {
            "Loading..."
        }
    }
}