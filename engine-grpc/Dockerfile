# engine-grpc/Dockerfile
FROM node:20-alpine AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY tsconfig.json jest.config.js ./
COPY proto ./proto
COPY src ./src
RUN npm run proto:gen && npm run build

FROM node:20-alpine AS runtime
WORKDIR /app
ENV NODE_ENV=production
COPY package*.json ./
RUN npm ci --omit=dev
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/proto ./proto
EXPOSE 50051
CMD ["node", "dist/server.js"]
