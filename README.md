# Ruggine - A Full-Stack Chat Application in Rust

Ruggine is a complete, real-time desktop chat application built entirely in Rust. It features a high-performance, asynchronous Axum backend and a responsive, native desktop client built with the `egui` framework.

---

## ✨ Features

* **Secure User Authentication**: User registration with `bcrypt` password hashing and session management using JSON Web Tokens (JWT).
* **Real-Time Group Chat**: Create groups and exchange messages instantly via a WebSocket connection.
* **Complete Invitation System**: Invite other users to join groups, view pending invitations, and accept or decline them.
* **Message History**: Automatically loads the most recent messages when you select a group chat.

---

## 🛠️ Tech Stack

### Backend

* **Framework**: **Axum** for handling REST API endpoints and WebSocket connections.
* **Async Runtime**: **Tokio** for high-performance, non-blocking I/O.
* **Database**: **SQLite** with **SQLx** for fully asynchronous, compile-time checked SQL queries.
* **State Management**: In-memory chat state managed by **DashMap** for highly concurrent access.
* **Serialization**: **Serde** for robust JSON data handling.

### Frontend

* **GUI**: **eframe** / **egui** for a simple, fast, and cross-platform native UI.
* **Architecture**: Multi-threaded design with a dedicated network thread to ensure the UI never freezes.
* **Networking**: **reqwest** for REST API calls and **tokio-tungstenite** for the WebSocket client.
