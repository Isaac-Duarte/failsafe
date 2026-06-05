# What is this
I want to have software where somebody can download it on multiple machines and it provides an apple-like experience when it comes to device syncing.

# Ideas I have
- Clipboard Syncing
- Copy and paste files
- Notification Syncing
- Shell Spawning
- Laptop as monitor
- Screen sharing
- Remote desktop
- TCP Reverse Proxy
- Virtual Shared Drive
- Camera / Microphone handoff
- Shared media controls

# Rough idea on how I will do it
I want to have a trait-based approach for each one of these features. For testing purposes I think I want to abstract all Iroh specific stuff to these, however, I think this may not be feasible?

I want a main web sever that will handle account & device registration.

One person can have multiple devices, and each device can pick an choose which feature they want to have enabled. As an optional feature, it would be cool to invoke all of these features from the web. Now Iroh does compile to web assembly but it doesn't look like its a direct connection anyway. I think how I would approach this instead is to open that direct connection from the sever itself, but only as a one way for limited features. Ie opening a shell, remote desktop, etc.

# TODO

- A better readme
