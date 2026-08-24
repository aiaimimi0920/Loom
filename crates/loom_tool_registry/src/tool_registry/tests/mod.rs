//! Focused tests for the split registry implementation.

use std::fs;
use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

pub(super) fn temp_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("loom-tool-registry-{name}-{nonce}"));
    fs::create_dir_all(&root).expect("create temp tool registry root");
    root
}

mod cloud_exec;
mod cloud_fixture;
mod cloud_policy;
mod image_budget;
mod image_exec;
mod image_fixtures;
mod image_security;
mod mcp;
mod mcp_fixture;
mod registry;
mod response;
mod validation;

use cloud_fixture::*;
use image_fixtures::*;
use image_security::{loopback_cloud_metadata, loopback_mcp_image_policy};
use mcp_fixture::*;
