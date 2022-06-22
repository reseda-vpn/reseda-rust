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
COPY --from=cacher /app/target target
RUN cargo build --release --bin reseda-rust

FROM ubuntu:latest

# set version label
ARG WIREGUARD_RELEASE="v1.0.20210914"
ARG COREDNS_VERSION

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

# RUN \
#  echo "**** install CoreDNS ****" && \
#  COREDNS_VERSION=$(curl -sX GET "https://api.github.com/repos/coredns/coredns/releases/latest" \
# 	| awk '/tag_name/{print $4;exit}' FS='[""]' | awk '{print substr($1,2); }') && \
#  curl -o \
# 	/tmp/coredns.tar.gz -L \
# 	"https://github.com/coredns/coredns/releases/download/v${COREDNS_VERSION}/coredns_${COREDNS_VERSION}_linux_amd64.tgz" && \
#  tar xf \
# 	/tmp/coredns.tar.gz -C \
# 	/app && 

RUN \
 echo "**** clean up ****" && \
 rm -rf \
	/tmp/* \
	/var/lib/apt/lists/* \
	/var/tmp/*

COPY --from=builder /app/target/release/ ./app

# Add Configuration File
ADD config.reseda ./app 
ADD .env ./app

# Add Certificates REQURED for HTTPS
ADD cert.pem ./app 
ADD key.pem ./app

# ports and volumes
EXPOSE 51820/udp
EXPOSE 80
EXPOSE 443

WORKDIR /app

RUN sudo update-alternatives --set iptables /usr/sbin/iptables-legacy

RUN mkdir ./configs
RUN sudo su - root

CMD ["sudo", "./reseda-rust"]