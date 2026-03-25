# swarmly

Reverse proxy for Docker Swarm with automatic TLS via Let's Encrypt. Routes traffic to containers and services based on labels, issues and renews certificates automatically, and supports multi-node deployments via Redis.

## How it works

Swarmly polls Docker every 10 seconds for services and containers with `swarmly.domain` labels. For each domain it measures TCP latency to all replicas and proxies requests to the closest one. When `ACME_EMAIL` is set, it obtains TLS certificates via ACME http-01 challenge automatically.

In a multi-node Swarm deployment, Redis is used to share certificates and coordinate issuance so only one node performs the ACME request per domain.

## Quick start

**Single node (development):**

```bash
docker compose -f compose.dev.yml up -d
```

**Docker Swarm (production):**

```bash
docker swarm init
docker stack deploy -c compose.prod.yml swarmly
```

## Routing

Swarmly discovers services and containers by reading their labels. The only required label is `swarmly.domain`.

### Docker Swarm services

```yaml
services:
  api:
    image: my-api:latest
    networks:
      - proxy-network
    deploy:
      replicas: 3
      labels:
        - swarmly.domain=api.example.com
        - swarmly.port=8080
```

### Plain Docker containers

```yaml
services:
  app:
    image: my-app:latest
    labels:
      - swarmly.domain=app.example.com
      - swarmly.port=3000
```

## Labels

| Label | Required | Default | Description |
|---|---|---|---|
| `swarmly.domain` | yes | — | Domain to route to this service |
| `swarmly.port` | no | `80` | Port the service listens on |
| `swarmly.tls` | no | `false` | Connect to the upstream over HTTPS |

### `swarmly.domain`

The domain that Swarmly will route to this service. Must match the DNS record pointing to your proxy.

```yaml
labels:
  - swarmly.domain=example.com
```

### `swarmly.port`

The port your container is listening on. Defaults to `80`.

```yaml
labels:
  - swarmly.domain=example.com
  - swarmly.port=8080
```

### `swarmly.tls`

Set to `true` if the upstream service itself serves HTTPS. Swarmly will connect to it over TLS and use the domain as SNI. Defaults to `false`.

```yaml
labels:
  - swarmly.domain=example.com
  - swarmly.port=443
  - swarmly.tls=true
```

## Environment variables

| Variable | Required | Description |
|---|---|---|
| `ACME_EMAIL` | no | Email for Let's Encrypt. Enables automatic TLS and HTTP→HTTPS redirect. |
| `REDIS_URL` | no | Redis connection URL. Enables distributed mode for multi-node Swarm deployments. |
| `DATA_DIR` | no | Directory for storing certificates when not using Redis. Defaults to `/opt/swarmly/certs`. |

### `ACME_EMAIL`

When set, Swarmly will:
- Obtain TLS certificates from Let's Encrypt for every routed domain
- Renew certificates automatically after 60 days
- Redirect all HTTP traffic to HTTPS (301)

```yaml
environment:
  - ACME_EMAIL=admin@example.com
```

ACME http-01 challenge traffic (`/.well-known/acme-challenge/`) is always handled by Swarmly itself and is never redirected.

### `REDIS_URL`

Required for multi-node Swarm deployments. Swarmly uses Redis to:
- Share certificates between nodes so every node can serve TLS without running its own ACME request
- Coordinate certificate issuance with a distributed lock so only one node contacts Let's Encrypt per domain

```yaml
environment:
  - REDIS_URL=redis://redis:6379
```

Without `REDIS_URL`, certificates are stored on the local filesystem under `DATA_DIR`.

### `DATA_DIR`

Path where certificates are stored when Redis is not configured.

```yaml
environment:
  - DATA_DIR=/data/certs
```

## Health check

Swarmly responds to health check requests on both port 80 and 443 without proxying them upstream.

```
GET /health   → 200 ok
GET /healthz  → 200 ok
```

## Ports

| Port | Description |
|---|---|
| `80` | HTTP. Proxies traffic or redirects to HTTPS when `ACME_EMAIL` is set. |
| `443` | HTTPS. Only active when `ACME_EMAIL` is set. |
| `7765` | Internal ACME challenge service. Not exposed externally. |

## Production setup

### compose.prod.yml

```yaml
services:
  redis:
    image: redis:7-alpine
    command: redis-server --appendonly yes
    volumes:
      - redis-data:/data
    networks:
      - proxy-network
    deploy:
      placement:
        constraints:
          - node.role == manager

  proxy:
    image: ghcr.io/magwoo/swarmly:latest
    ports:
      - 80:80
      - 443:443
    environment:
      - ACME_EMAIL=admin@example.com
      - REDIS_URL=redis://redis:6379
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    networks:
      - proxy-network
    depends_on:
      - redis
    deploy:
      replicas: 3

volumes:
  redis-data:

networks:
  proxy-network:
    name: proxy-network
```

### Example application service

```yaml
services:
  api:
    image: my-api:latest
    networks:
      - proxy-network
    deploy:
      replicas: 3
      labels:
        - swarmly.domain=api.example.com
        - swarmly.port=8080

networks:
  proxy-network:
    external: true
    name: proxy-network
```

The service must be on the same Docker network as the proxy. Swarmly detects which networks it belongs to and only routes to services on shared networks.

## Access log

Every proxied request is logged:

```
10.0.1.5 "GET api.example.com /users" 200 14ms
10.0.1.5 "POST api.example.com /orders" 201 42ms
```

Format: `{client_ip} "{method} {host} {path}" {status} {latency}ms`
