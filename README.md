Ruggine Chat
Ruggine is a full-stack, real-time chat application built entirely in Rust. It features a high-performance, asynchronous backend and a native, cross-platform desktop client.

The project demonstrates a complete client-server architecture with secure user authentication, group management, and real-time messaging.

Key Features
User Management: Secure user registration with bcrypt password hashing and session management using JWT.

Group Chat: Users can create groups, invite others, and communicate in real-time.

Invitation System: A full lifecycle for invitations, allowing users to send, view, accept, or decline group invites.

Real-Time Communication: Messaging is powered by WebSockets for low-latency, bidirectional communication.

Tech Stack
Backend:

Language: Rust

Web Framework: Axum

Async Runtime: Tokio

Database: SQLite with SQLx for compile-time checked queries.

In-Memory State: A concurrent DashMap manages the real-time chat state for active groups.

Frontend:

Language: Rust

GUI Framework: eframe / egui

Architecture: A multi-threaded design separates the UI from network operations (Reqwest, tokio-tungstenite) to ensure a responsive, non-blocking user experience.
