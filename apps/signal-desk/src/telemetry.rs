//! Deterministic samples for preview and interaction testing.
use std::fmt::Write;

pub struct Host {
    pub name: &'static str,
    pub hostname: &'static str,
    pub os: &'static str,
    pub uptime: &'static str,
    pub address: &'static str,
    pub memory: f32,
    pub storage: &'static [Storage],
    pub interfaces: &'static [(&'static str, f32, f32)],
}

pub struct Storage {
    pub name: &'static str,
    pub capacity: &'static str,
    pub used: f32,
}

pub const HOSTS: [Host; 2] = [
    Host {
        name: "Studio M2",
        hostname: "studio-m2.local",
        os: "macOS 15.6",
        uptime: "24d 2h",
        address: "192.0.2.42",
        memory: 16.,
        storage: &[
            Storage {
                name: "Macintosh HD",
                capacity: "52 / 494 GB",
                used: 10.5,
            },
            Storage {
                name: "Data",
                capacity: "278 / 494 GB",
                used: 56.3,
            },
            Storage {
                name: "Preboot",
                capacity: "2 / 16 GB",
                used: 12.5,
            },
            Storage {
                name: "Archive",
                capacity: "685 / 1,024 GB",
                used: 66.9,
            },
            Storage {
                name: "Workspace",
                capacity: "18 / 64 GB",
                used: 28.1,
            },
        ],
        interfaces: &[
            ("en0", 42.0, 75.5),
            ("utun4", 15.3, 15.6),
            ("lo0", 3.7, 3.7),
            ("bridge0", 0., 0.),
            ("awdl0", 0., 0.),
            ("llw0", 0., 0.),
            ("en1", 0., 0.),
            ("en2", 0., 0.),
            ("utun0", 0., 0.),
            ("utun1", 0., 0.),
        ],
    },
    Host {
        name: "Edge Node 01",
        hostname: "edge-01.lab",
        os: "Linux · Ubuntu 24.04",
        uptime: "7d 16h",
        address: "198.51.100.17",
        memory: 32.,
        storage: &[
            Storage {
                name: "/",
                capacity: "66 / 220 GB",
                used: 30.,
            },
            Storage {
                name: "/srv/data",
                capacity: "710 / 1,024 GB",
                used: 69.3,
            },
            Storage {
                name: "/boot",
                capacity: "0.3 / 2 GB",
                used: 15.,
            },
            Storage {
                name: "/var/lib",
                capacity: "28 / 60 GB",
                used: 46.7,
            },
        ],
        interfaces: &[
            ("eth0", 67.2, 28.4),
            ("wg0", 26.5, 18.6),
            ("lo", 5.8, 5.8),
            ("docker0", 0., 0.),
            ("eth1", 0., 0.),
            ("veth0", 0., 0.),
        ],
    },
];

pub struct Sample {
    pub host: &'static Host,
    pub load: f32,
    pub cpu_user: f32,
    pub cpu_system: f32,
    pub memory: f32,
    pub swap: f32,
    pub running: u32,
    pub receive: [f32; 60],
    pub transmit: [f32; 60],
}

impl Sample {
    pub fn new(host: usize, tick: usize, traffic: &Traffic) -> Self {
        let profile = &HOSTS[host];
        let drift = if tick == 0 {
            0.
        } else {
            (tick as f32 * 1.37).sin()
        };
        Self {
            host: profile,
            load: [6.83, 2.47][host] + drift * 0.29,
            cpu_user: [58.1, 34.5][host] + drift * 6.,
            cpu_system: [17.4, 10.3][host] - drift * 3.,
            memory: [12.8, 12.1][host] + drift * 0.21,
            swap: [0.9, 0.3][host],
            running: [64, 31][host],
            receive: traffic.receive,
            transmit: traffic.transmit,
        }
    }
}

pub struct Traffic {
    rng: u64,
    receive: [f32; 60],
    transmit: [f32; 60],
}

impl Traffic {
    pub fn new(host: &Host, seed: u64) -> Self {
        let (_, receive, transmit) = host.interfaces[0];
        let mut traffic = Self {
            rng: seed.max(1),
            receive: [receive; 60],
            transmit: [transmit; 60],
        };
        for _ in 0..60 {
            traffic.advance(host);
        }
        traffic.receive[59] = receive;
        traffic.transmit[59] = transmit;
        traffic
    }

    pub fn advance(&mut self, host: &Host) {
        let (_, receive, transmit) = host.interfaces[0];
        let received = next_rate(&mut self.rng, self.receive[59], receive);
        let transmitted = next_rate(&mut self.rng, self.transmit[59], transmit);
        self.receive.rotate_left(1);
        self.transmit.rotate_left(1);
        self.receive[59] = received;
        self.transmit[59] = transmitted;
    }
}

fn random_unit(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 48) as f32 / 65536.
}

fn next_rate(state: &mut u64, previous: f32, baseline: f32) -> f32 {
    let jitter = (random_unit(state) - 0.5) * 29.;
    let activity = random_unit(state);
    let burst = if activity > 0.88 { activity * 63. } else { 0. };
    (baseline * 0.46 + previous * 0.29 + jitter + burst).clamp(1., 160.)
}

pub fn traffic_path(samples: &[f32], stroke: &str) -> String {
    const HEIGHT: f32 = 54.;
    const PEAK: f32 = 160.;
    let mut path = String::with_capacity(samples.len() * 14);
    for (index, &value) in samples.iter().enumerate() {
        let x = index as f32 * 240. / (samples.len() - 1) as f32;
        let y = HEIGHT - 3. - (HEIGHT - 7.) * (value / PEAK);
        let _ = write!(path, "{}{x:.1},{y:.1}", if index == 0 { "M" } else { "L" });
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 54" preserveAspectRatio="none">
        <path d="M0 10H240 M0 30H240 M0 53H240" stroke="#344058" stroke-width=".65" opacity=".55"/>
        <path d="{path}L240,54H0Z" fill="{stroke}" opacity=".14"/>
        <path d="{path}" fill="none" stroke="{stroke}" stroke-width="1.55" stroke-linejoin="round" stroke-linecap="round"/>
        </svg>"##
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_traffic_appends_one_point_and_preserves_the_previous_history() {
        let mut first = Traffic::new(&HOSTS[0], 12);
        let mut same = Traffic::new(&HOSTS[0], 12);
        let mut different = Traffic::new(&HOSTS[0], 13);
        let old = first.receive;
        first.advance(&HOSTS[0]);
        same.advance(&HOSTS[0]);
        different.advance(&HOSTS[0]);

        assert_eq!(first.receive[..59], old[1..]);
        assert_eq!(first.receive, same.receive);
        assert_ne!(first.receive, different.receive);
        assert!((1. ..=160.).contains(&first.receive[59]));
    }
}
