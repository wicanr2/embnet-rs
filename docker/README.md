# 容器

所有建置與測試都在容器裡跑（`CLAUDE.md` 硬規則）。版本選擇的理由見 `docs/plans/004-p0-execution.md` 第二節。

| 映像 | Dockerfile | 基底與 digest |
|---|---|---|
| `embnet-rust:1.100.0-r1` | `rust.Dockerfile` | `rust:1.100.0-slim-bookworm@sha256:7c7d4eb4d05b7781187a327809466cfddf29753f6defdc3d19a0e291951b7a37` |
| `embnet-py:3.13-r1` | `py.Dockerfile` | `python:3.13.18-slim-trixie@sha256:e48f59cdba69a3cbd9ed2cf0f348d90d45a8d350d98134cf1070bc106ee9ed90` |
| `antmicro/renode:1.16.1` | 不另建 | `sha256:fc2a8c1bad2296a6d7cbc852bbf5540b22b778bdeb0ad42a45b8c54ea1e6a24c` |

建置：

```
docker build -t embnet-rust:1.100.0-r1 -f docker/rust.Dockerfile docker/
docker build -t embnet-py:3.13-r1 -f docker/py.Dockerfile docker/
```

crate 快取放在 docker volume `embnet-cargo`。改了 Dockerfile 就把 tag 的 `-rN` 加一。
