# Ghostscript for `make check-spot`, whose tiffsep device renders each plate of a PDF to a file of its own, the spot
# inks by name. Debian 12's package: the check reads what is printed on each plate, which doesn't move between versions.
FROM debian:12-slim

RUN apt-get update && apt-get install -y --no-install-recommends ghostscript && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

WORKDIR /work
ENTRYPOINT ["gs"]
