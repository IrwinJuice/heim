Simple service for deploy artifact on windows server.
It uses http for transfer files.
Can be used in case if SSH (SCP) is blocked by SecPolicy.

#### Routes
`/deploy`

    - (optional) run script before
    - move and unzip `<artifact>.zip` to `destination` path
    - (optional) run script after

