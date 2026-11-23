# syntax=docker/dockerfile:1
# Syntax sample: Dockerfile.
ARG NODE_VERSION=22
FROM --platform=linux/amd64 node:${NODE_VERSION}-alpine AS build
LABEL org.opencontainers.image.title="sample" maintainer='someone'
ENV NODE_ENV=production \
    PORT=3000
WORKDIR /app
COPY --chown=node:node package.json bun.lock ./
RUN --mount=type=cache,target=/root/.cache npm ci && \
    npm run build
COPY . .

FROM nginx:stable AS runtime
EXPOSE 3000/tcp
USER node
VOLUME ["/data"]
HEALTHCHECK --interval=30s CMD curl -f http://localhost:$PORT/ || exit 1
ONBUILD RUN echo "child build"
COPY --from=build /app/dist /usr/share/nginx/html
ENTRYPOINT ["nginx", "-g", "daemon off;"]
CMD ["--help"]
