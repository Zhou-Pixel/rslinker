# Rslinker
rslinker is a NAT traversal tool.

# Configuration

## TCP
> Data is not encrypted
1. Server
```toml
[server]
port = 33445
protocol = "tcp"
```

2. Client
```toml
[[client]]
server_addr = "xxx.xxx.xxx.xxx"
server_port = 33445
protocol = "tcp"

[[client.link]] #example
local_addr = "127.0.0.1"
local_port = 22
remote_port = 55323
protocol = "ssh"

[[client.link]] #example
local_addr = "192.168.31.101"
local_port = 80
remote_port = 5525
protocol = "tcp"

```
## TLS
1. Server
```toml
[server]
port = 33445
protocol = "tls"
[server.tls_config]
cert = "./certs/server.cert"
key = "./certs/server.key"
enable_client_auth = false # Same as the client
```

2. Client
```toml
[[client]]
server_addr = "xxx.xxx.xxx.xxx"
server_port = 33445
protocol = "tls"

[[client.link]] #example
local_addr = "127.0.0.1"
local_port = 22
remote_port = 55323
protocol = "ssh" # or tcp is ok

[[client.link]] #example
local_addr = "192.168.31.101"
local_port = 80
remote_port = 5525
protocol = "tcp"

[client.tls_config]
ca = "./certs/ca.cert"
server_name = "localhost"
enable_client_auth = false # Same as the client
```

## QUIC
1. Server
```toml
[server]
port = 33445
protocol = "quic"
[server.tls_config]
cert = "./certs/server.cert"
key = "./certs/server.key"
enable_client_auth = false # Same as the Client
```

2. Client
```toml
[[client]]
server_addr = "xxx.xxx.xxx.xxx"
server_port = 33445
protocol = "quic"

[[client.link]] #example
local_addr = "127.0.0.1"
local_port = 22
remote_port = 55323
protocol = "ssh" # or tcp is ok

[[client.link]] #example
local_addr = "192.168.31.101"
local_port = 80
remote_port = 5525
protocol = "tcp"

[client.quic_config]
ca = "./certs/ca.cert"
server_name = "localhost"
enable_client_auth = false
```

## All Options
1. Server
```toml
[server]
port = 33445
addr = "0.0.0.0"
protocol = "quic"

[server.tcp_config]
nodelay = true

[server.quic_config]
cert = "./certs/server.cert"
key = "./certs/server.key"
enable_client_auth = true
ca = "./certs/ca.cert"

[server.tls_config]
cert = "./certs/server.cert"
key = "./certs/server.key"
enable_client_auth = true
ca = "./certs/ca.cert"
```

2. Client
```toml
[[client]] # First Client
server_addr = "127.0.0.1"
server_port = 33445
accept_conflict = false # Multiple clients may have conflicts
retry_times = 0
heartbeat_interval = 1000 # ms
protocol = "quic"

[[client.link]] #example
local_addr = "127.0.0.1"
local_port = 22
remote_port = 55323
protocol = "ssh" # or tcp is ok

[[client.link]] #example 
local_addr = "192.168.31.101"
local_port = 80
remote_port = 5525
protocol = "tcp"

[client.tcp_config]
nodelay = false

[client.quic_config]
cert = "./certs/client.cert"
key = "./certs/client.key"
ca = "./certs/ca.cert"
server_name = "localhost"
enable_client_auth = true

[client.tls_config]
cert = "./certs/client.cert"
key = "./certs/client.key"
ca = "./certs/ca.cert"
server_name = "localhost"
enable_client_auth = true
```

# Command
1. run as a server
```bash
rslinker run --server # Default config path: ./server.toml
```
2. run as a client
```bash
rslinker run --client # Default config path: ./client.toml
```