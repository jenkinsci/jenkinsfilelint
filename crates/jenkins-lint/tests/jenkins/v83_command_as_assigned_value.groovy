pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script { x = foo bar }
            }
        }
    }
}
