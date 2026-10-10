//! Prints the GraphQL SDL. Bazel turns this into the schema the frontend's Relay compiler reads.

fn main() {
    print!("{}", awzy_corp_portal::schema().sdl());
}
