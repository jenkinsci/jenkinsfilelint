pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script { "a${b}" { x() } }
            }
        }
    }
}
