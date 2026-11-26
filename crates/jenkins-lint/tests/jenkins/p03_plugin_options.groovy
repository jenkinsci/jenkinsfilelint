pipeline {
    agent any
    options {
        timestamps()
        ansiColor('xterm')
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
