node('linux') {
    docker.image('python').inside {
        sh 'pytest'
    }
    def db = docker.image('localhost:5000/postgres')
    db.withRun('-e POSTGRES_PASSWORD=test') { c ->
        sh 'make integration-test'
    }
}
