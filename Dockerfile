FROM node:22-bookworm AS ui-builder
WORKDIR /app/ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci
COPY ui/ ./
RUN VITE_TRANSPORT=http npm run build

FROM rust:1.88-bookworm AS rust-builder
WORKDIR /app
COPY . .
RUN cargo build -p audit-agent-web --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /app/target/release/audit-agent-web /usr/local/bin/audit-agent-web
COPY --from=ui-builder /app/ui/dist /usr/share/audit-agent/web

EXPOSE 3000
ENV WORK_DIR=/data
VOLUME ["/data"]

CMD ["audit-agent-web", "--port", "3000", "--work-dir", "/data", "--static-dir", "/usr/share/audit-agent/web"]
