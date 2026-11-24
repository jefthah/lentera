# Image server Lentera: program Go statis di atas distroless (tanpa shell, user
# nonroot). Migrasi database ikut tertanam di program.
#
#   docker build -t lentera-server server
#
# Di VPS image dibangun lewat server/deploy/deploy.sh (lihat deploy/README.md).

FROM golang:1.26.5-alpine AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -trimpath -ldflags='-s -w' -o /out/lentera-server ./cmd/lentera-server

# static-debian12 sudah berisi sertifikat CA (untuk API Resend) dan data zona waktu.
FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /out/lentera-server /lentera-server
ENV APP_ENV=production \
    HTTP_ADDR=0.0.0.0:8310
EXPOSE 8310
USER nonroot:nonroot
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/lentera-server", "health"]
ENTRYPOINT ["/lentera-server"]
CMD ["serve"]
