//! Names for generated people: varied, common, and fictional in combination.

const GIVEN: &str = "Aaliyah Adrian Aisha Alejandro Amara Andrei Anika Arjun Beatriz Ben Camila Chen Chloe \
    Daniel Daria David Elena Emeka Emily Farah Felix Gabriel Grace Hana Hugo Imani Isaac Ivy Jamal Jin Jonas \
    Julia Kai Kavya Laila Leo Lucia Malik Maya Mei Mateo Nadia Nikolai Noah Olivia Omar Priya Rafael Rosa \
    Ruth Samir Sara Sebastian Sofia Tariq Theo Uma Victor Yara Zoe";

const FAMILY: &str = "Abara Alvarez Andersen Baptiste Bauer Bose Castillo Chowdhury Costa Dimitrov Dubois \
    Eriksen Ferreira Fischer Gallagher Garcia Haddad Hayashi Hernandez Ibrahim Ivanova Jensen Kapoor Kim \
    Kowalczyk Larsen Lindqvist Mendes Moreau Murphy Nakamura Nguyen Novak Oduya Olsen Ortiz Park Petrov \
    Quinn Rahman Reyes Rossi Santos Schmidt Shah Silva Sokolov Suzuki Tanaka Torres Varga Vasquez Wagner \
    Walsh Weber Yilmaz Young Zhang Zielinski Zubiri";

pub fn given() -> Vec<&'static str> {
    GIVEN.split_whitespace().collect()
}

pub fn family() -> Vec<&'static str> {
    FAMILY.split_whitespace().collect()
}
