import socket
import threading
import os
import subprocess
from cryptography.fernet import Fernet
import uuid

def install_dependencies():
    required = {'cryptography'}
    installed = {pkg.key for pkg in pkg_resources.working_set}
    missing = required - installed

    if missing:
        subprocess.check_call([sys.executable, '-m', 'pip', 'install', *missing])

try:
    import pkg_resources
except ImportError:
    subprocess.check_call([sys.executable, '-m', 'pip', 'install', 'setuptools'])
    import pkg_resources

install_dependencies()

def derive_key(user_key):
    return user_key  # Directly using the provided key for simplicity

def join_common_chat():
    ngrok_address = input("Enter ngrok address (e.g., 0.tcp.in.ngrok.io): ").strip()
    ngrok_port = int(input("Enter ngrok port (e.g., 12654): ").strip())
    user_key = input("Enter encryption key: ").strip()
    encryption_key = derive_key(user_key)

    client_name = input("Enter your name: ").strip()
    if not client_name:
        print("Name cannot be empty")
        exit(1)

    client_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)

    try:
        client_socket.connect((ngrok_address, ngrok_port))
        encryption_key = client_socket.recv(1024)  # Receive encryption key from server
        client_socket.sendall(client_name.encode())
    except Exception as e:
        print(f"Failed to connect to server: {e}")
        exit(1)

    cipher_suite = Fernet(encryption_key)

    # Display message for exiting the chat
    print("Type /!exit!/ to Exit the Chat")

    def receive_messages():
        while True:
            try:
                encrypted_message = client_socket.recv(1024)
                if not encrypted_message:
                    break
                message = cipher_suite.decrypt(encrypted_message).decode()
                print(f"\r{message}\n({client_name}): ", end="", flush=True)
            except:
                print("\nExited the Chat")
                break
        client_socket.close()

    def send_message():
        while True:
            message = input(f"({client_name}): ").strip()
            if not message:
                continue
            encrypted_message = cipher_suite.encrypt(message.encode())
            client_socket.sendall(encrypted_message)
            if message == "/!exit!/":
                break
        client_socket.close()

    if __name__ == "__main__":
        receive_thread = threading.Thread(target=receive_messages)
        receive_thread.start()
        send_message()

def create_session():
    pass  # Implement session creation logic

def join_session():
    pass  # Implement session joining logic

def main():
    print("Options:")
    print("1) Join the common chat")
    print("2) Create a session")
    print("3) Join a session")

    choice = input("Enter your choice: ")
    if choice == "1":
        join_common_chat()
    elif choice == "2":
        create_session()
    elif choice == "3":
        join_session()
    else:
        print("Invalid choice")

if __name__ == "__main__":
    main()
