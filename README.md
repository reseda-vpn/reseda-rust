# reseda-rust
[![Build Docker image and Push to Docker Hub.](https://github.com/bennjii/reseda-rust/actions/workflows/docker.yml/badge.svg)](https://github.com/bennjii/reseda-rust/actions/workflows/docker.yml)
 
This is the `node` module for the reseda network. It interfaces with the [`reseda-mesh`](https://github.com/bennjii/reseda-mesh) network to instantiate itself and acts as an unknowing peer.
By this, its location and data management are held by the mesh, and not by the node. 

The client connects to the node directly, bypassing the need for the mesh to organise and determine connections. This **signficantly** increases the time-to-connection. 
This project was previously named [`reseda-server`](https://github.com/bennjii/reseda-server). However, this public archive shows the origional implementation was achieved 
using typescript. Although this was not the worst choice. It most certainly was not the best. Encoutering many continuity errors and a severe case of single-threadedness.

Thus, the reseda-rust implementation rose to take its place. This implementation is far better produced, and has a substaintially higher code and logic quality. This implementation is significantly faster and more robust.
Built entirely using rust, it utilizes the multithreaded nature of rust to analyse, monitor and manage a linux container running [`wireguard`](https://www.wireguard.com/).



> This module is not intended for client deployment.
