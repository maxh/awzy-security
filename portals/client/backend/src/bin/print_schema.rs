//! Prints the GraphQL SDL. Bazel turns this into the schema the frontend's Relay compiler reads.

fn main() {
    print!("{}", awzy_client_portal::schema().sdl());
}
