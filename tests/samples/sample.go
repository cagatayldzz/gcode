// Syntax sample: Go.
package main

import "fmt"

type User struct {
	ID   int
	Name string
}

func (u User) Greet() string {
	return fmt.Sprintf("Hello, %s!", u.Name)
}

func main() {
	users := []User{{1, "ada"}, {2, "bob"}}
	for _, user := range users {
		fmt.Println(user.Greet())
	}
}
