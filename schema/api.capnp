@0xfd92ce9be2369b8e;

struct Maybe(Value) {
    union {
        some @0 :Value;
        none @1 :Void;
    }
}

struct Either(Left, Right) {
    union {
        left @0 :Left;
        right @1 :Right;
    }
}

struct Subject {
    id @0 :Text;
    domain @1 :Text;
}

struct Machine {
    name @0 :Text;
    location @1 :Text;
    status @2 :Status;
}

enum Status {
    free @0;
    occupied @1;
    blocked @2;
}

interface BffhAdmin {
    getAllSubjects @0 () -> (subjects :List(Subject) );

    getAllMachines @1 () -> (machines :List(Machine) );
    addMachine @2 (name :Text, location :Text ) -> ();

    machineSetState @3 (name :Text, state :Status ) -> ();

    authentication @4 () -> ( auth :Authentication );
}

interface Permissions {
    getAllSubjects @0 () -> (subjects :List(Subject) );
}

interface Notification {
    machineChangeState @0 (machine :Machine ) -> ();
}

interface Authentication {
    # List all SASL mechs the server is willing to use
    availableMechanisms @0 () -> ( mechanisms :List(Text) );

    # Start authentication using the given mechanism and optional initial data
    initializeAuthentication @1 ( mechanism :Text, initialData :Maybe(Data) )
        -> (response :Either (Challenge, Outcome) );

    getAuthzid @2 () -> ( authzid :Text );

    interface Challenge {
        # Access the challenge data
        read @0 () -> ( data :Maybe(Data) );

        respond @1 ( data :Maybe(Data) ) 
            -> ( response :Either (Challenge, Outcome) );
    }

    interface Outcome {
        # Outcomes may contain additional data
        read @0 () -> ( data :Maybe(Data) );
        # The actual outcome.
        value @1 () -> ( granted :Bool );
    }
}
