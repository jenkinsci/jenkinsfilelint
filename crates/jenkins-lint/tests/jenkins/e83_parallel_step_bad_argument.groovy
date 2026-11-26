pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                parallel('a', 'b')
            }
        }
    }
}
