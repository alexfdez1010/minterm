use serde::Serialize;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
pub struct GitInfo {
    pub branch: String,
    pub dirty: Option<bool>,
    pub detached: bool,
}

#[derive(Debug, Serialize)]
pub struct Port {
    pub port: u16,
    pub process: String,
    pub pid: u32,
    pub kind: Option<String>,
    pub address: String,
}

#[derive(Debug, Serialize)]
pub struct Database {
    pub name: String,
    pub image: String,
    pub ports: String,
    pub state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Services {
    pub ports: Vec<Port>,
    pub databases: Vec<Database>,
    pub ports_error: Option<String>,
    pub docker_error: Option<String>,
}

// Inspector children can stall on a daemon or filesystem. Read while they run so
// a full output pipe cannot deadlock a scan. The deadline covers descendants too.
fn capture(command: &mut Command, timeout: Duration) -> io::Result<(ExitStatus, String)> {
    let (mut reader, writer) = UnixStream::pair()?;
    let stderr = writer.try_clone()?;
    let mut child = command
        .env("LC_ALL", "C")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::from(OwnedFd::from(stderr)))
        .spawn()?;
    // Command retains its descriptors for reuse. Release those copies for EOF.
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let deadline = Instant::now() + timeout;
    let remaining = || {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
    };
    let result = (|| {
        let mut bytes = Vec::new();
        let mut buf = [0; 8192];
        loop {
            reader.set_read_timeout(Some(remaining()?))?;
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) if bytes.len() + n <= 2 * 1024 * 1024 => bytes.extend_from_slice(&buf[..n]),
                Ok(_) => return Err(io::Error::other("Inspector output exceeded 2 MiB")),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok((status, String::from_utf8_lossy(&bytes).into_owned()));
            }
            std::thread::sleep(remaining()?.min(Duration::from_millis(5)));
        }
    })();
    if result.is_err() {
        // SAFETY: the unreaped child is leader of the group created above.
        unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
        let _ = child.wait();
    }
    result
}

fn git_command(cwd: &Path) -> Command {
    let mut command = Command::new("/usr/bin/git");
    command
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["-c", "core.fsmonitor=false"]);
    command
}

pub fn git(cwd: &Path) -> Option<GitInfo> {
    let status = capture(
        git_command(cwd).args([
            "status",
            "--porcelain=v2",
            "--branch",
            "--no-ahead-behind",
            "--untracked-files=no",
            "--ignore-submodules=untracked",
        ]),
        Duration::from_millis(1200),
    );
    if let Ok((status, output)) = status {
        return status.success().then(|| parse_git(&output)).flatten();
    }
    // A large worktree may take too long to inspect. Keep its branch visible.
    let (ok, branch) = capture(
        git_command(cwd).args(["symbolic-ref", "--short", "-q", "HEAD"]),
        Duration::from_millis(350),
    )
    .ok()?;
    let detached = !ok.success();
    let branch = if detached {
        let (ok, oid) = capture(
            git_command(cwd).args(["rev-parse", "--short", "HEAD"]),
            Duration::from_millis(350),
        )
        .ok()?;
        ok.success().then_some(oid)?
    } else {
        branch
    };
    Some(GitInfo {
        branch: branch.trim().to_owned(),
        dirty: None,
        detached,
    })
}

fn parse_git(output: &str) -> Option<GitInfo> {
    let head = output
        .lines()
        .find_map(|s| s.strip_prefix("# branch.head "))?;
    let detached = head == "(detached)";
    let branch = if detached {
        let oid = output
            .lines()
            .find_map(|s| s.strip_prefix("# branch.oid "))?;
        oid.get(..8)?
    } else {
        head
    };
    Some(GitInfo {
        branch: branch.to_owned(),
        dirty: Some(output.lines().any(|line| {
            line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ")
        })),
        detached,
    })
}

// These names describe common port conventions. The process comes from lsof.
fn common_use(port: u16) -> Option<&'static str> {
    Some(match port {
        3000..=3003 | 4000 | 5000..=5001 | 8000..=8001 | 8080..=8081 | 8888 => "Web dev",
        4173..=4174 | 5173..=5174 => "Vite",
        4200 => "Angular",
        4321 => "Astro",
        5555 => "Prisma Studio",
        6006 => "Storybook",
        9229 => "Node inspector",
        11434 => "Ollama",
        1433 => "SQL Server",
        1521 => "Oracle",
        3306 => "MySQL / MariaDB",
        5432 | 54322 => "PostgreSQL",
        6379 => "Redis / Valkey",
        7474 | 7687 => "Neo4j",
        8123 | 9000 => "ClickHouse",
        9042 => "Cassandra",
        9200 => "Elasticsearch",
        11211 => "Memcached",
        26257 => "CockroachDB",
        27017..=27019 => "MongoDB",
        _ => return None,
    })
}

fn parse_ports(output: &str) -> Vec<Port> {
    let mut ports = Vec::new();
    let mut pid = None;
    let mut process = "";
    for field in output.split('\0').map(|f| f.trim_start_matches('\n')) {
        if let Some(value) = field.strip_prefix('p') {
            pid = value.parse().ok();
            process = "";
        } else if let Some(value) = field.strip_prefix('c') {
            process = value;
        } else if let (Some(endpoint), Some(pid)) = (field.strip_prefix('n'), pid) {
            let Some((address, port)) = endpoint.rsplit_once(':') else {
                continue;
            };
            let Ok(port) = port.parse::<u16>() else {
                continue;
            };
            ports.push(Port {
                port,
                address: address.to_owned(),
                pid,
                process: process.to_owned(),
                kind: common_use(port).map(str::to_owned),
            });
        }
    }
    ports.sort_unstable_by(|a, b| {
        (a.kind.is_none(), a.port, a.pid, &a.address).cmp(&(
            b.kind.is_none(),
            b.port,
            b.pid,
            &b.address,
        ))
    });
    ports.dedup_by(|a, b| a.port == b.port && a.address == b.address && a.pid == b.pid);
    ports
}

fn listeners() -> Result<Vec<Port>, String> {
    let (status, output) = capture(
        Command::new("/usr/sbin/lsof").args(["-nP", "+c", "0", "-iTCP", "-sTCP:LISTEN", "-Fpcn0"]),
        Duration::from_secs(3),
    )
    .map_err(|_| "Could not read TCP listeners in time.".to_owned())?;
    if status.success() || (status.code() == Some(1) && output.trim().is_empty()) {
        Ok(parse_ports(&output))
    } else {
        Err("Could not read TCP listeners.".to_owned())
    }
}

fn docker_binary() -> Option<PathBuf> {
    // Finder launches with a shorter PATH than an interactive shell.
    let search = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&search)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join("docker"))
        .chain(env::var_os("HOME").map(|home| PathBuf::from(home).join(".docker/bin/docker")))
        .chain([
            PathBuf::from("/opt/homebrew/bin/docker"),
            PathBuf::from("/usr/local/bin/docker"),
            PathBuf::from("/Applications/Docker.app/Contents/Resources/bin/docker"),
        ])
        .find(|path| fs::metadata(path).is_ok_and(|m| m.is_file()))
}

fn is_database_image(image: &str) -> bool {
    let reference = image.split('@').next().unwrap_or(image);
    let component = reference.rsplit('/').next().unwrap_or(reference);
    let name = component.split(':').next().unwrap_or(component);
    matches!(
        name,
        "postgres"
            | "postgresql"
            | "postgresql-repmgr"
            | "pgvector"
            | "postgis"
            | "timescaledb"
            | "timescaledb-ha"
            | "mysql"
            | "mysql-server"
            | "mariadb"
            | "mariadb-galera"
            | "percona"
            | "percona-server"
            | "mongo"
            | "mongodb"
            | "mongodb-community-server"
            | "redis"
            | "redis-stack"
            | "redis-stack-server"
            | "valkey"
            | "keydb"
            | "dragonfly"
            | "memcached"
            | "clickhouse-server"
            | "cassandra"
            | "scylla"
            | "cockroach"
            | "neo4j"
            | "elasticsearch"
            | "opensearch"
            | "influxdb"
            | "couchdb"
            | "couchbase"
            | "dynamodb-local"
            | "yugabyte"
            | "surrealdb"
            | "qdrant"
            | "weaviate"
            | "milvus"
            | "chromadb"
            | "oracle-free"
            | "oracle-xe"
    ) || (reference.rsplit('/').nth(1) == Some("mssql") && name == "server")
}

fn parse_databases(output: &str) -> Vec<Database> {
    let mut databases = Vec::new();
    for row in output.lines() {
        let mut fields = row.splitn(4, '\t');
        if let (Some(name), Some(image), Some(state), Some(ports)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
            && is_database_image(image)
        {
            databases.push(Database {
                name: name.to_owned(),
                image: image.to_owned(),
                state: state.to_owned(),
                ports: ports.to_owned(),
            });
        }
    }
    databases.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    databases
}

fn prioritize_database_ports(ports: &mut [Port], databases: &[Database]) {
    let published: Vec<_> = databases
        .iter()
        .flat_map(|db| db.ports.split(','))
        .filter_map(|mapping| {
            let (host, target) = mapping.trim().split_once("->")?;
            if !target.ends_with("/tcp") {
                return None;
            }
            let (_, binding) = host.rsplit_once(':')?;
            let (start, end) = binding.split_once('-').unwrap_or((binding, binding));
            Some(start.parse::<u16>().ok()?..=end.parse::<u16>().ok()?)
        })
        .collect();
    for port in ports.iter_mut() {
        if matches!(
            port.process.as_str(),
            "com.docker.backend" | "docker-proxy" | "vpnkit" | "gvproxy"
        ) && published.iter().any(|range| range.contains(&port.port))
        {
            port.kind = Some("Docker DB".to_owned());
        }
    }
    ports.sort_by_key(|p| (p.kind.is_none(), p.port));
}

fn databases() -> Result<Vec<Database>, String> {
    let docker = docker_binary().ok_or("Docker CLI is unavailable.")?;
    let context_selected = env::var_os("DOCKER_CONTEXT").is_some_and(|v| !v.is_empty());
    let host = env::var("DOCKER_HOST").ok().filter(|s| !s.is_empty());
    let endpoint = if let Some(host) = host.filter(|_| !context_selected) {
        host
    } else {
        let (status, host) = capture(
            Command::new(&docker).args([
                "context",
                "inspect",
                "--format",
                "{{.Endpoints.docker.Host}}",
            ]),
            Duration::from_secs(2),
        )
        .map_err(|_| "Could not read the Docker context.")?;
        if !status.success() {
            return Err("Could not read the Docker context.".to_owned());
        }
        host.trim().to_owned()
    };
    if !endpoint.starts_with("unix://") {
        return Err("Select a local Docker context to inspect containers.".to_owned());
    }
    let (status, output) = capture(
        Command::new(docker)
            .env_remove("DOCKER_CONTEXT")
            .env_remove("DOCKER_HOST")
            .args([
                "--host",
                &endpoint,
                "ps",
                "--format",
                "{{.Names}}\t{{.Image}}\t{{.State}}\t{{.Ports}}",
            ]),
        Duration::from_secs(3),
    )
    .map_err(|_| "Docker did not respond in time.")?;
    if !status.success() {
        return Err("Could not reach the local Docker daemon.".to_owned());
    }
    Ok(parse_databases(&output))
}

pub fn services() -> Services {
    let (ports, databases) = std::thread::scope(|s| {
        let ports = s.spawn(listeners);
        let databases = databases();
        (
            ports.join().expect("listener inspector panicked"),
            databases,
        )
    });
    let ports_error = ports.as_ref().err().cloned();
    let docker_error = databases.as_ref().err().cloned();
    let mut ports = ports.unwrap_or_default();
    let databases = databases.unwrap_or_default();
    prioritize_database_ports(&mut ports, &databases);
    Services {
        ports,
        databases,
        ports_error,
        docker_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listeners_prioritize_known_ports_and_preserve_bind_addresses() {
        let ports = parse_ports(
            "p20\0cagent\0\nf4\0n127.0.0.1:59955\0\np21\0cnode\0\nf4\0n*:5173\0\nf8\0n*:5173\0\nf9\0n[::1]:5173\0\n",
        );
        assert_eq!(ports.len(), 3);
        assert_eq!((ports[0].port, ports[0].pid), (5173, 21));
        assert_eq!(ports[0].kind.as_deref(), Some("Vite"));
        assert_eq!(ports[1].address, "[::1]");
        assert_eq!(ports[2].process, "agent");
    }

    #[test]
    fn detached_and_unborn_branches_handle_tracked_status() {
        let git = parse_git("# branch.oid (initial)\n# branch.head main\n").unwrap();
        assert_eq!(git.branch, "main");
        assert_eq!(git.dirty, Some(false));
        let git =
            parse_git("# branch.oid 1234567890abcd\n# branch.head (detached)\n2 R. N... rename\n")
                .unwrap();
        assert!(git.detached);
        assert_eq!(git.branch, "12345678");
        assert_eq!(git.dirty, Some(true));
    }

    #[test]
    fn docker_matches_database_images_and_keeps_actual_mappings() {
        let dbs = parse_databases(
            "finance\tregistry:5000/team/pgvector:16\trunning\t0.0.0.0:5434->5432/tcp\ncache\tvalkey/valkey@sha256:aabbcc\trunning\t\nadmin\tdpage/pgadmin4\trunning\t80/tcp\n",
        );
        assert_eq!(dbs.len(), 2);
        assert_eq!(dbs[0].name, "cache");
        assert!(dbs[0].ports.is_empty());
        assert_eq!(dbs[1].ports, "0.0.0.0:5434->5432/tcp");
        assert!(is_database_image(
            "mcr.microsoft.com/mssql/server:2022-latest"
        ));
        assert!(!is_database_image("example/server:latest"));
        let mut ports = parse_ports("p4\0cnode\0n*:5400\0\np9\0ccom.docker.backend\0n*:5434\0");
        prioritize_database_ports(&mut ports, &dbs);
        assert_eq!(ports[0].port, 5434);
        assert_eq!(ports[0].kind.as_deref(), Some("Docker DB"));
    }

    #[test]
    fn inspector_deadline_also_reaps_silent_children() {
        let start = Instant::now();
        assert!(
            capture(
                Command::new("/bin/sleep").arg("5"),
                Duration::from_millis(60)
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
