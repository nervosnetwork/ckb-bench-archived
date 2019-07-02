use crate::config::Url;
use serde_derive::{Deserialize, Serialize};
use std::time::SystemTime;

pub struct Netdata {
    urls: Vec<Url>,
    client: reqwest::Client,
}

impl Netdata {
    pub fn new(urls: Vec<Url>) -> Self {
        Self {
            urls,
            client: reqwest::Client::new(),
        }
    }

    #[allow(dead_code)]
    pub fn cpu(&self, start: SystemTime, end: SystemTime) -> Vec<CPU> {
        let mut results = Vec::new();
        for url in self.urls.iter() {
            let cpu_url = cpu_url(url, start, end);
            let mut response = self.client.get(&cpu_url).send().expect("FIXME");

            let cpu: CPU = response.json().expect("FIXME");
            results.push(cpu);
        }
        results
    }

    #[allow(dead_code)]
    pub fn memory(&self, start: SystemTime, end: SystemTime) -> Vec<Memory> {
        let mut results = Vec::new();
        for url in self.urls.iter() {
            let memory_url = memory_url(url, start, end);
            let mut response = self.client.get(&memory_url).send().expect("FIXME");

            let memory: Memory = response.json().expect("FIXME");
            results.push(memory);
        }
        results
    }
}

// TODO refactor
fn cpu_url(base_url: &Url, _start: SystemTime, _end: SystemTime) -> String {
    //    let points = 1;
    //    let params = [
    //        ("chart", "system.cpu"),
    //        ("after", &format!("{}", after)),
    //        ("after", "-400"),
    //        ("points", &format!("{}", points)),
    //        ("group", "average"),
    //        ("format", "json"),
    //        ("gtime", "0"),
    //        ("options", "seconds,jsonwrap")
    //    ];

    // Too short, so that netdata will return zero points
    //    let after = start.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    //    let before = end.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let after = -30;
    format!(
        "{}/api/v1/data?chart=system.cpu&after={}&points=1&group=average&gtime=0&format=json&options=seconds,jsonwrap",
        base_url.as_str(),
        after,
    )
}

fn memory_url(base_url: &Url, _start: SystemTime, _end: SystemTime) -> String {
    let after = -30;
    format!(
        "{}/api/v1/data?chart=mem.pgfaults&after={}&points=1&group=average&gtime=0&format=json&options=seconds,jsonwrap",
        base_url.as_str(),
        after,
    )
}

// TODO add hostname
#[derive(Deserialize, Serialize)]
pub struct CPU {
    after: u64,
    before: u64,
    #[serde(rename = "result")]
    points: Points,
}

#[derive(Deserialize, Serialize)]
pub struct Memory {
    after: u64,
    before: u64,
    #[serde(rename = "result")]
    points: Points,
}

#[derive(Deserialize, Serialize)]
pub struct Points {
    labels: Vec<String>,
    data: Vec<Vec<f32>>,
}
