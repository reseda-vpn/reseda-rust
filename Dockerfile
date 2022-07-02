FROM rust:1.61 as planner

WORKDIR /app

RUN cargo install cargo-chef 
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.61 as cacher
WORKDIR /app
RUN cargo install cargo-chef
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM rust:1.61 as builder
WORKDIR /app
COPY . .

ARG db
ENV DATABASE_URL=$db

COPY --from=cacher /app/target target
RUN cargo build --release --bin reseda-rust

FROM ubuntu:latest

# set version label
ARG WIREGUARD_RELEASE="v1.0.20210914"

RUN \
 mkdir /app \
 echo "**** install dependencies ****" && \
 apt-get update && \
 apt-get install -y --no-install-recommends \
    libc6 \
    sudo \
	bc \
	build-essential \
	curl \
	dkms \
	git \
	gnupg \ 
	ifupdown \
	iproute2 \
	iptables \
	iputils-ping \
	jq \
	libelf-dev \
	net-tools \
	openresolv \
	perl \
	pkg-config \
	qrencode \
	ca-certificates

RUN \
 echo "**** install wireguard-tools ****" && \
 if [ -z ${WIREGUARD_RELEASE+x} ]; then \
	WIREGUARD_RELEASE=$(curl -sX GET "https://api.github.com/repos/WireGuard/wireguard-tools/tags" \
	| jq -r .[0].name); \
 fi && \
 cd /app && \
 GIT_SSL_NO_VERIFY=1 git clone https://git.zx2c4.com/wireguard-linux-compat && \
 GIT_SSL_NO_VERIFY=1 git clone https://git.zx2c4.com/wireguard-tools && \
 echo "**** finishing with ${WIREGUARD_RELEASE} update ****" && \
 cd wireguard-tools && \
 git checkout "${WIREGUARD_RELEASE}" && \
 make -C src -j$(nproc) && \
 make -C src install 

RUN \
 echo "**** clean up ****" && \
 rm -rf \
	/tmp/* \
	/var/lib/apt/lists/* \
	/var/tmp/*

COPY --from=builder /app/target/release/reseda-rust ./app

# ports and volumes
EXPOSE 8443/udp
EXPOSE 80
EXPOSE 443

ARG mesh_auth
ARG access_key

RUN mkdir ./app/configuration
RUN echo 'mesh_auth: "${mesh_auth}" \ndatabase_auth: "${db}"\n access_key: "${access_key}"' > ./app/configuration/base.yml

WORKDIR /app

RUN sudo update-alternatives --set iptables /usr/sbin/iptables-legacy

RUN mkdir ./configs
RUN sudo su - root

CMD ["sudo", "./reseda-rust"]