@0xfd92ce9be2369b8e;

interface BffhAdmin {
    getAllSubjects @0 () -> (subjects :List(Subject));

    struct Subject {
        id @0 :Text;
        domain @1 :Text;
    }
}
