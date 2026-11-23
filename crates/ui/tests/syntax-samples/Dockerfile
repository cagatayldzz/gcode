# Syntax sample: Dockerfile.
FROM node:22-alpine AS build
WORKDIR /app
COPY package.json ./
RUN npm install && npm run build
ENV NODE_ENV=production
EXPOSE 3000
CMD ["node", "dist/index.js"]
