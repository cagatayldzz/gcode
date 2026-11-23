// Syntax sample: Go.
package main

import (
	"errors"
	"fmt"
	"os"
	str "strings"
)

const Limit = 16

var ErrMissing = errors.New("missing")

type Role int

const (
	Admin Role = iota
	Guest
)

type User struct {
	ID    int    `json:"id"`
	Name  string `json:"name"`
	Roles []Role
}

type Greeter interface {
	Greet(prefix string) string
}

func (u *User) Greet(prefix string) string {
	return fmt.Sprintf("%s, %s!", prefix, u.Name)
}

func find[T any](items []T, keep func(T) bool) (T, error) {
	var zero T
	for _, item := range items {
		if keep(item) {
			return item, nil
		}
	}
	return zero, ErrMissing
}

/* A block
   comment. */
func main() {
	users := []User{{ID: 1, Name: "ada"}, {ID: 2, Name: "bob"}}
	lookup := map[string]*User{}
	ch := make(chan int, 2)
	raw := `raw
string`
	r := 'x'
	go func() { ch <- 1 }()
	defer close(ch)
	for i, user := range users {
		lookup[str.ToUpper(user.Name)] = &users[i]
		switch {
		case user.ID > 1:
			fmt.Println(user.Greet("Hello"), raw, r, 0xFF, 3.5e2)
		default:
			continue
		}
	}
	if _, err := find(users, func(u User) bool { return u.ID == 3 }); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
