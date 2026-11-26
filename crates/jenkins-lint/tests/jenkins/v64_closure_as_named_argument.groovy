pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                foo(onFail: { x = 1 })
            }
        }
    }
}
