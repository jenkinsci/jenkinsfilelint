pipeline {
    agent any
    environment {
        FOO = ['a', 'b']
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
