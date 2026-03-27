FROM scratch
COPY escli /escli
ENTRYPOINT ["/escli"]
