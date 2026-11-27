pipeline {
    agent any
    environment {
        FOO
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
