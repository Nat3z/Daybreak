package main

import (
	"fmt"
	"net"
	"time"
)

func main() {
	conn, err := net.Dial("tcp", "127.0.0.1:8080")
	if err != nil {
		fmt.Printf("Failed to connect: %v\n", err)
		return
	}
	conn.Write([]byte{2, 3})
	connError := conn.Close()
	if connError != nil {
		fmt.Printf("Failed to close connection: %v\n", connError)
	}

	time.Sleep(2 * time.Second)
	conn, err = net.Dial("tcp", "127.0.0.1:8080")
	if err != nil {
		fmt.Printf("Failed to connect: %v\n", err)
		return
	}
	conn.Write([]byte{3, 3})
	connError = conn.Close()
	if connError != nil {
		fmt.Printf("Failed to close connection: %v\n", connError)
	}
	fmt.Println("Connection closed")
}
