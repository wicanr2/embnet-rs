# embnet-renode：Renode 1.17.0 portable 版。Docker Hub 上最新的編號 tag 是 1.16.1，
# 它沒有 nucleo_f767zi.repl，所以改用 GitHub release 1.17.0 自建（docs/plans/004 第二節）。
# 基底借用已鎖版的 python slim 映像，因為它內含 .NET 執行需要的 libssl。
FROM python:3.13.18-slim-trixie@sha256:e48f59cdba69a3cbd9ed2cf0f348d90d45a8d350d98134cf1070bc106ee9ed90

# 不裝 libicu：.NET 以 invariant globalization 執行即可。
ENV DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1

ADD --checksum=sha256:4ba7c68b59e2447f188ef4b4b112fcccd2802582c460beedceb63b84e5605a5f \
    https://github.com/renode/renode/releases/download/v1.17.0/renode-1.17.0.linux-portable.tar.gz /tmp/renode.tar.gz

RUN mkdir -p /opt/renode \
 && tar -xzf /tmp/renode.tar.gz -C /opt/renode --strip-components=1 \
 && rm /tmp/renode.tar.gz \
 && ln -s /opt/renode/renode /usr/local/bin/renode
