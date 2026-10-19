FROM node:22.14-alpine AS frontend-build
WORKDIR /app
COPY frontend/ .
RUN npm install -g pnpm@12.9.0 \
    && pnpm install --frozen-lockfile \
    && pnpm build

FROM rust:1.97-alpine AS backend-build
RUN apk add --no-cache musl-dev && mkdir -p /app/.local
WORKDIR /app
COPY backend/ .
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=backend-build /app/target/x86_64-unknown-linux-musl/release/bookmark-backend /bookmark-backend
COPY --from=backend-build --chown=65534:65534 /app/.local /app/.local
COPY --from=frontend-build /app/dist /frontend/dist
ENV FRONTEND_DIR=/frontend/dist
ENV DATABASE_PATH=/app/.local/bookmark.db
ENV TMPDIR=/app/.local
USER 65534
EXPOSE 8989
ENTRYPOINT ["/bookmark-backend"]
