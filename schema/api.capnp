# Copyright © 2020 Gregor Reitzenstein
# Licensed under the MIT License:
#
# Permission is hereby granted, free of charge, to any person obtaining
# a copy of this software and associated documentation files (the "Software"),
# to deal in the Software without restriction, including without limitation
# the rights to use, copy, modify, merge, publish, distribute, sublicense,
# and/or sell copies of the Software, and to permit persons to whom the
# Software is furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included
# in all copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
# EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
# OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
# IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
# DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
# TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE
# OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.


@0xfd92ce9be2369b8e;

interface Diflouroborane {
    # Upon initial connection this is the interface a program is presented with, serving as the
    # common point to access specific subsystems. Keep in mind that one can use pipelining to make this
    # just as efficient as direct calls — e.g. access the authentication system and call
    # `initializeAuthentication` on it in one roundtrip, provided one gets granted access to the
    # Authentication subsystem (which in all fairness is a reasonable assumption)

    authentication @0 () -> ( auth :Authentication );
    # Then authentication subsystem handles authentication of clients and servers. Multiple
    # authentication is possible, see the `Authentication` interface for details.

    permissions @1 () -> ( perm :Permissions );
    # Permission subsystem to manage permissions and systems underlying the authorization process

    machines @2 () -> ( mach :Machines );
    # Diflouroborane stores machine¹ information in an opaque internal database. This interface is
    # the only stable process of modifying that information

    # TODO Capability transfer system, required for machine takeover, session resumption.
}

struct Maybe(Value) {
    # An optional value, i.e. a value which is either explicity present or explicity not present.
    # Similar to `Maybe` in Haskell and `Option` in OCaml or Rust
    union {
        some @0 :Value;
        none @1 :Void;
    }
}

struct Either(Left, Right) {
    # Sum type over two values. A more general type than Rust's `Result` type.
    # If this type is used to convey the result of a possibly failed computation the `Left` type
    # shall be used for the error while the `Right` type shall be the value. (Mnemonic: 'right' also
    # means 'correct')
    union {
        left @0 :Left;
        right @1 :Right;
    }
}

struct UUID {
    lsg @0 :UInt64; # least significant
    msg @1 :UInt64; # most significant
}

interface Machines {
    interface Manage {
        setBlocked @0 ( blocked :Bool ) -> ();
        # Block or Unblock the machine. A blocked machine can not be used.

        return @1 () -> ();
        # Forcefully marking a machine as `returned` — i.e. not used.
    }

    interface Return {
        # The only way of getting a `return` interface is by successfully calling `use`. This means
        # only the user that marked a machine as `used` can return it again. (Baring force override)
        return @0 () -> ();
    }

    manage @0 ( uuid :UUID ) -> ( manage :Manage );

    use @1 ( uuid :UUID ) -> ( return :Return );
    # Use a machine, identified by its UUID. If the caller is allowed to and the machine is
    # available to being used a `return` Capability will be returned — the person using a machine is
    # after all the only person that can return the machine after use.
}

interface Permissions {
    getAllSubjects @0 () -> ( subjects :List(Text) );
    getAllObjects @1 () -> ( objects :List(Text) );
    getAllAction @2 () -> ( actions :List(Text) );
    getAllRoles @3 () -> ( roles :List(Text) );

    removePolicy @4 ( p :List(Text) ) -> ();
    addPolicy @5 ( p :List(Text) ) -> ();
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
