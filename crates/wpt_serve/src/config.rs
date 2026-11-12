//! wptserve's default server config, with fixed values for the ports it
//! allocates dynamically ("auto").

use std::collections::HashMap;
use std::sync::LazyLock;

/// wptserve's default `browser_host`
pub const BROWSER_HOST: &str = "web-platform.test";
/// wptserve's default `alternate_hosts["alt"]`
pub const ALT_HOST: &str = "not-web-platform.test";
/// The origin tests are loaded from (`http://{{host}}:{{ports[http][0]}}`)
pub const ORIGIN: &str = "http://web-platform.test:8000/";

pub(crate) const PORTS: &[(&str, &[u16])] = &[
    ("http", &[8000, 8001]),
    ("http-local", &[8002]),
    ("http-public", &[8003]),
    ("https", &[8443, 8444]),
    ("https-local", &[8445]),
    ("https-public", &[8446]),
    ("h2", &[9000]),
    ("ws", &[8888]),
    ("wss", &[8889]),
    ("webtransport-h3", &[11000]),
    ("websocket-h3", &[11001]),
    ("dns", &[8053]),
];

/// wptserve's `subdomains` labels, with their IDNA encodings
const SUBDOMAIN_LABELS: &[(&str, &str)] = &[
    ("www", "www"),
    ("www1", "www1"),
    ("www2", "www2"),
    ("天気の良い日", "xn--n8j6ds53lwwkrqhv28a"),
    ("élève", "xn--lve-6lad"),
];
/// wptserve's `not_subdomains` labels
const NOT_SUBDOMAIN_LABELS: &[(&str, &str)] = &[("nonexistent", "nonexistent")];

/// Subdomain name -> IDNA-encoded subdomain, for wptserve's `subdomains` and
/// `not_subdomains` (each label set combined up to two levels deep)
pub(crate) static SUBDOMAINS: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for labels in [SUBDOMAIN_LABELS, NOT_SUBDOMAIN_LABELS] {
        for (name, encoded) in labels {
            map.insert(name.to_string(), encoded.to_string());
            for (name2, encoded2) in labels {
                map.insert(format!("{name}.{name2}"), format!("{encoded}.{encoded2}"));
            }
        }
    }
    map
});
