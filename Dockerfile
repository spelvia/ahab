FROM golang:1.26-alpine AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
ARG VERSION=dev
RUN CGO_ENABLED=0 go build -trimpath \
    -ldflags "-s -w -X main.version=${VERSION}" \
    -o /out/ahab ./cmd/ahab

FROM alpine:3.21
# ahab shells out to cluster CLIs; kubectl and helm ship in the image.
# flux and argocd are not packaged in Alpine — mount or bake them in if needed.
RUN apk add --no-cache ca-certificates kubectl helm
COPY --from=build /out/ahab /usr/local/bin/ahab
WORKDIR /workspace
ENTRYPOINT ["ahab"]
