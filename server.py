import socket
import threading
from cryptography.fernet import Fernet
import time
import uuid
import random

# Generate a key for encryption
encryption_key = Fernet.generate_key()
cipher_suite = Fernet(encryption_key)

# Dictionary to store clients and their session details
clients = {}

def get_host_port():
    host = input("Enter the host (e.g., 127.0.0.1): ").strip()
    port = int(input("Enter the port (e.g., 3000): ").strip())
    return host, port

def get_server_socket(host, port):
    server_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server_socket.bind((host, port))
    server_socket.listen(5)
    return server_socket

def handle_client(client_socket, client_address):
    try:
        client_socket.sendall(encryption_key)  # Send encryption key to client
        session_id = str(uuid.uuid4())  # Generate unique session ID
        session_key = Fernet.generate_key()  # Generate session-specific encryption key
        clients[client_socket] = (session_id, session_key)
        client_socket.sendall(session_id.encode())  # Send session ID to client
        client_socket.sendall(session_key)  # Send session-specific encryption key to client

        client_name = client_socket.recv(1024).decode()
        if not client_name:
            client_socket.close()
            return

        welcome_message = f"--- {client_name} has joined the chat ---"
        broadcast(welcome_message, client_socket)

        while True:
            encrypted_message = client_socket.recv(1024)
            if not encrypted_message:
                break
            message = cipher_suite.decrypt(encrypted_message).decode()
            if message == "/!exit!/":
                break
            else:
                formatted_message = f"{client_name}: {message}"
                broadcast(formatted_message, client_socket)
    except:
        pass
    finally:
        if client_socket in clients:
            session_id, _ = clients[client_socket]
            del clients[client_socket]
            client_socket.close()
            goodbye_message = f"--- {client_name} has left the chat ---"
            broadcast(goodbye_message, client_socket)

def broadcast(message, sender_socket):
    for client_socket in list(clients):
        if client_socket != sender_socket:
            try:
                encrypted_message = cipher_suite.encrypt(message.encode())
                client_socket.sendall(encrypted_message)
            except:
                client_socket.close()
                if client_socket in clients:
                    del clients[client_socket]

def main():
    print("Server setup:")
    host, port = get_host_port()
    server_socket = get_server_socket(host, port)

    print("Server is running...")
    print(f"Encryption key: {encryption_key.decode()}")  # Print the encryption key for the client
    try:
        while True:
            client_socket, client_address = server_socket.accept()
            thread = threading.Thread(target=handle_client, args=(client_socket, client_address))
            thread.start()
    except KeyboardInterrupt:
        print("\nServer is shutting down...")
    finally:
        server_socket.close()
        for client_socket in clients:
            client_socket.close()

if __name__ == "__main__":
    main()
