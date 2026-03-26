# syntax=docker/dockerfile:1
FROM scratch
COPY escli /escli
ENTRYPOINT ["/escli"]
